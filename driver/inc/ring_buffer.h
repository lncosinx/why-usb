#pragma once

#include <stdint.h>
#include <atomic>
#include <cstddef>

// Since we'll compile this both on Linux (for tests) and Windows (for the actual driver),
// we use standard atomics which compile everywhere. In actual Windows kernel development,
// Interlocked operations might be necessary depending on the IRQL.

constexpr size_t RING_BUFFER_SIZE = 1024 * 1024; // 1MB Ring Buffer

struct SPSC_RingBuffer {
    std::atomic<size_t> head; // Written by Producer
    std::atomic<size_t> tail; // Written by Consumer
    uint8_t data[RING_BUFFER_SIZE];

    SPSC_RingBuffer() : head(0), tail(0) {}

    // Producer writes data
    bool push(const uint8_t* src, size_t len) {
        size_t current_tail = tail.load(std::memory_order_acquire);
        size_t current_head = head.load(std::memory_order_relaxed);

        size_t available_space = RING_BUFFER_SIZE - (current_head - current_tail);
        if (len > available_space) {
            return false; // Not enough space
        }

        size_t offset = current_head % RING_BUFFER_SIZE;
        size_t first_part = len < (RING_BUFFER_SIZE - offset) ? len : (RING_BUFFER_SIZE - offset);

        // Copy first part
        for (size_t i = 0; i < first_part; ++i) {
            data[offset + i] = src[i];
        }

        // Copy second part if wrapping around
        if (first_part < len) {
            for (size_t i = 0; i < len - first_part; ++i) {
                data[i] = src[first_part + i];
            }
        }

        head.store(current_head + len, std::memory_order_release);
        return true;
    }

    // Consumer reads data
    bool pop(uint8_t* dst, size_t len) {
        size_t current_head = head.load(std::memory_order_acquire);
        size_t current_tail = tail.load(std::memory_order_relaxed);

        size_t available_data = current_head - current_tail;
        if (len > available_data) {
            return false; // Not enough data
        }

        size_t offset = current_tail % RING_BUFFER_SIZE;
        size_t first_part = len < (RING_BUFFER_SIZE - offset) ? len : (RING_BUFFER_SIZE - offset);

        // Copy first part
        for (size_t i = 0; i < first_part; ++i) {
            dst[i] = data[offset + i];
        }

        // Copy second part if wrapping around
        if (first_part < len) {
            for (size_t i = 0; i < len - first_part; ++i) {
                dst[first_part + i] = data[i];
            }
        }

        tail.store(current_tail + len, std::memory_order_release);
        return true;
    }
};

struct SharedMemoryContext {
    SPSC_RingBuffer tx_ring; // Driver -> Daemon
    SPSC_RingBuffer rx_ring; // Daemon -> Driver
};
