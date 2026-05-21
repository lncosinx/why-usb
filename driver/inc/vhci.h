#pragma once

#include <stdint.h>
#include <cstddef>
#include "ioctl.h"

#ifdef _WIN32
// For a mock Windows build without the actual WDK installed, we define the missing types.
// If compiling with WDK, these would be provided by ntddk.h and wdf.h.
#ifndef NTSTATUS
#define NTSTATUS int32_t
#endif
#ifndef STATUS_SUCCESS
#define STATUS_SUCCESS 0
#endif
#ifndef STATUS_UNSUCCESSFUL
#define STATUS_UNSUCCESSFUL -1
#endif
#else
// Mock declarations for non-Windows builds (like our Linux CI/testing environment)
#define NTSTATUS int32_t
#define STATUS_SUCCESS 0
#define STATUS_UNSUCCESSFUL -1

// Mock WDF structures
typedef void* WDFDRIVER;
typedef void* WDFDEVICE;
typedef void* WDFREQUEST;
typedef void* WDFMEMORY;

// Mock pool type
typedef int POOL_TYPE;
#define NonPagedPool 0

// Mock functions
inline void* ExAllocatePoolWithTag(POOL_TYPE, size_t, uint32_t) { return nullptr; }
inline void ExFreePool(void*) {}

#endif

// Forward declaration of the shared memory context
struct SharedMemoryContext;

// Public FFI functions
int32_t init_vhci_driver();
void cleanup_vhci_driver();
void use_external_shared_memory_context(SharedMemoryContext* context);

// FFI wrappers for ring buffer interaction
bool tx_ring_pop_frame(uint8_t* dst, size_t max_len, size_t* out_len);
bool rx_ring_push_frame(const uint8_t* src, size_t len);
bool get_shared_memory_info(WhyUsbSharedMemoryInfo* info);
bool get_driver_status(WhyUsbStatusResponse* status);

// Mock URB interception function
bool intercept_urb(const uint8_t* urb_data, size_t length);
bool mock_driver_pump_once();
