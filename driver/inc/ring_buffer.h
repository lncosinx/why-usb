#pragma once

#include <stdint.h>
#include <atomic>
#include <cstddef>
#include "ioctl.h"

// Since we'll compile this both on Linux (for tests) and Windows (for the actual driver),
// we use standard atomics which compile everywhere. In actual Windows kernel development,
// Interlocked operations might be necessary depending on the IRQL.

constexpr size_t RING_BUFFER_SIZE = WHY_USB_RING_BUFFER_SIZE; // 1MB Ring Buffer

struct SPSC_RingBuffer {
    std::atomic<uint64_t> head; // Written by Producer
    std::atomic<uint64_t> tail; // Written by Consumer
    uint8_t data[RING_BUFFER_SIZE];

    SPSC_RingBuffer() : head(0), tail(0) {}

    size_t available_data() const {
        uint64_t current_head = head.load(std::memory_order_acquire);
        uint64_t current_tail = tail.load(std::memory_order_acquire);
        return static_cast<size_t>(current_head - current_tail);
    }

    size_t available_space() const {
        return RING_BUFFER_SIZE - available_data();
    }

    bool push_frame(const uint8_t* src, size_t len) {
        if (len > UINT32_MAX || len + sizeof(uint32_t) < len) {
            return false;
        }

        uint8_t prefix[sizeof(uint32_t)] = {
            static_cast<uint8_t>((len >> 24) & 0xff),
            static_cast<uint8_t>((len >> 16) & 0xff),
            static_cast<uint8_t>((len >> 8) & 0xff),
            static_cast<uint8_t>(len & 0xff),
        };

        uint64_t current_tail = tail.load(std::memory_order_acquire);
        uint64_t current_head = head.load(std::memory_order_relaxed);
        size_t total_len = sizeof(uint32_t) + len;

        size_t available_space = RING_BUFFER_SIZE - static_cast<size_t>(current_head - current_tail);
        if (total_len > available_space) {
            return false; // Not enough space
        }

        copy_into(current_head, prefix, sizeof(uint32_t));
        copy_into(current_head + sizeof(uint32_t), src, len);

        head.store(current_head + static_cast<uint64_t>(total_len), std::memory_order_release);
        return true;
    }

    bool pop_frame(uint8_t* dst, size_t max_len, size_t* out_len) {
        uint64_t current_head = head.load(std::memory_order_acquire);
        uint64_t current_tail = tail.load(std::memory_order_relaxed);
        size_t available_data = static_cast<size_t>(current_head - current_tail);

        if (available_data < sizeof(uint32_t)) {
            return false;
        }

        uint32_t frame_len = peek_u32(current_tail);
        size_t total_len = sizeof(uint32_t) + static_cast<size_t>(frame_len);

        if (available_data < total_len || frame_len > max_len) {
            return false;
        }

        copy_from(current_tail + sizeof(uint32_t), dst, frame_len);
        tail.store(current_tail + static_cast<uint64_t>(total_len), std::memory_order_release);

        if (out_len) {
            *out_len = frame_len;
        }

        return true;
    }

    // Producer writes raw bytes. Kept for low-level tests; data-plane users should prefer push_frame.
    bool push(const uint8_t* src, size_t len) {
        uint64_t current_tail = tail.load(std::memory_order_acquire);
        uint64_t current_head = head.load(std::memory_order_relaxed);

        size_t available_space = RING_BUFFER_SIZE - static_cast<size_t>(current_head - current_tail);
        if (len > available_space) {
            return false; // Not enough space
        }

        copy_into(current_head, src, len);
        head.store(current_head + static_cast<uint64_t>(len), std::memory_order_release);
        return true;
    }

    // Consumer reads raw bytes. Kept for low-level tests; data-plane users should prefer pop_frame.
    bool pop(uint8_t* dst, size_t len) {
        uint64_t current_head = head.load(std::memory_order_acquire);
        uint64_t current_tail = tail.load(std::memory_order_relaxed);

        size_t available_data = static_cast<size_t>(current_head - current_tail);
        if (len > available_data) {
            return false; // Not enough data
        }

        copy_from(current_tail, dst, len);
        tail.store(current_tail + static_cast<uint64_t>(len), std::memory_order_release);
        return true;
    }

    // Consumer reads up to max_len bytes, returns how many bytes were actually read.
    // Kept for low-level tests; data-plane users should prefer pop_frame.
    size_t pop_some(uint8_t* dst, size_t max_len) {
        uint64_t current_head = head.load(std::memory_order_acquire);
        uint64_t current_tail = tail.load(std::memory_order_relaxed);

        size_t available_data = static_cast<size_t>(current_head - current_tail);
        if (available_data == 0) {
            return 0; // No data available
        }

        size_t len = max_len < available_data ? max_len : available_data;
        copy_from(current_tail, dst, len);
        tail.store(current_tail + static_cast<uint64_t>(len), std::memory_order_release);
        return len;
    }

private:
    void copy_into(size_t absolute_offset, const uint8_t* src, size_t len) {
        if (len == 0) {
            return;
        }

        size_t offset = absolute_offset % RING_BUFFER_SIZE;
        size_t first_part = len < (RING_BUFFER_SIZE - offset) ? len : (RING_BUFFER_SIZE - offset);

        for (size_t i = 0; i < first_part; ++i) {
            data[offset + i] = src[i];
        }

        if (first_part < len) {
            for (size_t i = 0; i < len - first_part; ++i) {
                data[i] = src[first_part + i];
            }
        }
    }

    void copy_from(size_t absolute_offset, uint8_t* dst, size_t len) const {
        if (len == 0) {
            return;
        }

        size_t offset = absolute_offset % RING_BUFFER_SIZE;
        size_t first_part = len < (RING_BUFFER_SIZE - offset) ? len : (RING_BUFFER_SIZE - offset);

        for (size_t i = 0; i < first_part; ++i) {
            dst[i] = data[offset + i];
        }

        if (first_part < len) {
            for (size_t i = 0; i < len - first_part; ++i) {
                dst[first_part + i] = data[i];
            }
        }
    }

    uint32_t peek_u32(size_t absolute_offset) const {
        uint8_t bytes[sizeof(uint32_t)] = {};
        copy_from(absolute_offset, bytes, sizeof(uint32_t));

        return (static_cast<uint32_t>(bytes[0]) << 24) |
               (static_cast<uint32_t>(bytes[1]) << 16) |
               (static_cast<uint32_t>(bytes[2]) << 8) |
               static_cast<uint32_t>(bytes[3]);
    }

};

struct SharedMemoryContext {
    WhyUsbSharedMemoryHeader header;
    SPSC_RingBuffer tx_ring; // Driver -> Daemon
    SPSC_RingBuffer rx_ring; // Daemon -> Driver

    SharedMemoryContext() : header{} {
        header.magic = WHY_USB_SHARED_MEMORY_MAGIC;
        header.version = WHY_USB_SHARED_MEMORY_VERSION;
        header.header_size = sizeof(WhyUsbSharedMemoryHeader);
        header.mapping_size = sizeof(SharedMemoryContext);
        header.tx_ring_offset = offsetof(SharedMemoryContext, tx_ring);
        header.rx_ring_offset = offsetof(SharedMemoryContext, rx_ring);
        header.tx_ring_size = sizeof(SPSC_RingBuffer);
        header.rx_ring_size = sizeof(SPSC_RingBuffer);
    }
};

static_assert(sizeof(SPSC_RingBuffer) == WHY_USB_RING_MAPPING_SIZE, "Ring ABI size mismatch");
