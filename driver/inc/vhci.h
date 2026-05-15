#pragma once

#include <stdint.h>
#include <cstddef>

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

// FFI wrappers for ring buffer interaction
size_t tx_ring_pop_some(uint8_t* dst, size_t max_len);
bool rx_ring_push(const uint8_t* src, size_t len);

// Mock URB interception function
bool intercept_urb(const uint8_t* urb_data, size_t length);
