#include "ring_buffer.h"

#include <algorithm>
#include <cassert>
#include <cstdint>
#include <iostream>
#include <vector>

namespace {

void expect_frame(SPSC_RingBuffer& ring, const std::vector<uint8_t>& expected)
{
    std::vector<uint8_t> actual(expected.size());
    size_t out_len = 0;

    assert(ring.pop_frame(actual.data(), actual.size(), &out_len));
    assert(out_len == expected.size());
    assert(actual == expected);
}

void preserves_frame_boundaries()
{
    SPSC_RingBuffer ring;
    std::vector<uint8_t> first = {0x01, 0x02, 0x03};
    std::vector<uint8_t> second = {0x10, 0x11, 0x12, 0x13};

    assert(ring.push_frame(first.data(), first.size()));
    assert(ring.push_frame(second.data(), second.size()));

    expect_frame(ring, first);
    expect_frame(ring, second);
    assert(!ring.pop_frame(first.data(), first.size(), nullptr));
}

void wraps_at_buffer_end()
{
    SPSC_RingBuffer ring;
    constexpr uint64_t near_end = RING_BUFFER_SIZE - 2;
    std::vector<uint8_t> payload = {0xde, 0xad, 0xbe, 0xef, 0x42};

    ring.head.store(near_end, std::memory_order_relaxed);
    ring.tail.store(near_end, std::memory_order_relaxed);

    assert(ring.push_frame(payload.data(), payload.size()));
    assert(ring.head.load(std::memory_order_relaxed) ==
           near_end + sizeof(uint32_t) + payload.size());

    expect_frame(ring, payload);
    assert(ring.available_data() == 0);
}

void rejects_oversized_frame()
{
    SPSC_RingBuffer ring;
    std::vector<uint8_t> payload(RING_BUFFER_SIZE - sizeof(uint32_t) + 1, 0xaa);

    assert(!ring.push_frame(payload.data(), payload.size()));
    assert(ring.head.load(std::memory_order_relaxed) == 0);
    assert(ring.tail.load(std::memory_order_relaxed) == 0);
}

void allows_exact_capacity_frame()
{
    SPSC_RingBuffer ring;
    std::vector<uint8_t> payload(RING_BUFFER_SIZE - sizeof(uint32_t), 0x7f);
    std::vector<uint8_t> small = {0x01};

    assert(ring.push_frame(payload.data(), payload.size()));
    assert(ring.available_space() == 0);
    assert(!ring.push_frame(small.data(), small.size()));

    expect_frame(ring, payload);
    assert(ring.available_space() == RING_BUFFER_SIZE);
}

void keeps_tail_when_destination_is_too_small()
{
    SPSC_RingBuffer ring;
    std::vector<uint8_t> payload = {0x20, 0x21, 0x22, 0x23};
    std::vector<uint8_t> too_small(2);
    uint64_t initial_tail = 0;

    assert(ring.push_frame(payload.data(), payload.size()));
    initial_tail = ring.tail.load(std::memory_order_relaxed);

    assert(!ring.pop_frame(too_small.data(), too_small.size(), nullptr));
    assert(ring.tail.load(std::memory_order_relaxed) == initial_tail);

    expect_frame(ring, payload);
}

void handles_many_small_frames()
{
    SPSC_RingBuffer ring;
    std::vector<std::vector<uint8_t>> frames;

    for (uint16_t i = 0; i < 1000; ++i) {
        frames.push_back({
            static_cast<uint8_t>((i >> 8) & 0xff),
            static_cast<uint8_t>(i & 0xff),
            static_cast<uint8_t>((i * 31) & 0xff),
        });
    }

    for (const auto& frame : frames) {
        assert(ring.push_frame(frame.data(), frame.size()));
    }

    for (const auto& frame : frames) {
        expect_frame(ring, frame);
    }

    assert(ring.available_data() == 0);
}

} // namespace

int main()
{
    preserves_frame_boundaries();
    wraps_at_buffer_end();
    rejects_oversized_frame();
    allows_exact_capacity_frame();
    keeps_tail_when_destination_is_too_small();
    handles_many_small_frames();

    std::cout << "ring_buffer_tests passed\n";
    return 0;
}
