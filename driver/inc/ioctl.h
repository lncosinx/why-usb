#pragma once

#include <stdint.h>

#ifndef FILE_DEVICE_UNKNOWN
#define FILE_DEVICE_UNKNOWN 0x00000022
#endif

#ifndef METHOD_BUFFERED
#define METHOD_BUFFERED 0
#endif

#ifndef FILE_ANY_ACCESS
#define FILE_ANY_ACCESS 0
#endif

#ifndef CTL_CODE
#define CTL_CODE(DeviceType, Function, Method, Access) \
    (((DeviceType) << 16) | ((Access) << 14) | ((Function) << 2) | (Method))
#endif

#define WHY_USB_ABI_MAGIC 0x31594857u // "WHY1", little-endian
#define WHY_USB_ABI_VERSION 1u
#define WHY_USB_RING_BUFFER_SIZE (1024u * 1024u)
#define WHY_USB_RING_HEADER_SIZE 16u
#define WHY_USB_RING_ALIGNMENT 8u
#define WHY_USB_RING_MAPPING_SIZE (WHY_USB_RING_HEADER_SIZE + WHY_USB_RING_BUFFER_SIZE)
#define WHY_USB_SHARED_MEMORY_MAGIC 0x4d535957u // "WYSM", little-endian
#define WHY_USB_SHARED_MEMORY_VERSION 1u

#define IOCTL_WHY_USB_SESSION_OPEN \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_WHY_USB_SESSION_CLOSE \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_WHY_USB_GET_SHARED_MEMORY \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_WHY_USB_ATTACH_DEVICE \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x804, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_WHY_USB_DETACH_DEVICE \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x805, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_WHY_USB_GET_STATUS \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS)

enum WHY_USB_STATUS : uint32_t {
    WHY_USB_STATUS_OK = 0,
    WHY_USB_STATUS_UNSUPPORTED = 1,
    WHY_USB_STATUS_INVALID_STATE = 2,
    WHY_USB_STATUS_BUFFER_TOO_SMALL = 3,
};

enum WHY_USB_SESSION_STATE : uint32_t {
    WHY_USB_SESSION_CLOSED = 0,
    WHY_USB_SESSION_OPEN = 1,
    WHY_USB_SESSION_ATTACHED = 2,
};

#pragma pack(push, 1)

struct WhyUsbAbiHeader {
    uint32_t magic;
    uint16_t version;
    uint16_t size;
};

struct WhyUsbSessionOpenRequest {
    WhyUsbAbiHeader header;
    uint32_t flags;
};

struct WhyUsbSessionOpenResponse {
    WhyUsbAbiHeader header;
    uint64_t session_id;
    uint32_t status;
    uint32_t max_frame_size;
};

struct WhyUsbSharedMemoryInfo {
    WhyUsbAbiHeader header;
    uint64_t section_handle;
    uint64_t tx_event_handle;
    uint64_t rx_event_handle;
    uint32_t mapping_size;
    uint32_t tx_ring_size;
    uint32_t rx_ring_size;
    uint32_t tx_ring_offset;
    uint32_t rx_ring_offset;
};

struct WhyUsbSharedMemoryHeader {
    uint32_t magic;
    uint16_t version;
    uint16_t header_size;
    uint32_t mapping_size;
    uint32_t tx_ring_offset;
    uint32_t rx_ring_offset;
    uint32_t tx_ring_size;
    uint32_t rx_ring_size;
};

struct WhyUsbAttachDeviceRequest {
    WhyUsbAbiHeader header;
    uint64_t session_id;
    uint16_t vendor_id;
    uint16_t product_id;
    uint8_t bus_id;
    uint8_t port_id;
    uint16_t flags;
};

struct WhyUsbDetachDeviceRequest {
    WhyUsbAbiHeader header;
    uint64_t session_id;
    uint32_t reason;
};

struct WhyUsbStatusResponse {
    WhyUsbAbiHeader header;
    uint64_t session_id;
    uint32_t status;
    uint32_t session_state;
    uint32_t tx_queued_bytes;
    uint32_t rx_queued_bytes;
};

#pragma pack(pop)
