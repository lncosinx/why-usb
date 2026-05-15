#include "vhci.h"
#include "ring_buffer.h"

// Global context pointer to simulate driver extension
static SharedMemoryContext* g_SharedMemory = nullptr;

#ifdef _WIN32
// In a real Windows build, this uses ExAllocatePoolWithTag
#include <ntddk.h>
#define VHCI_POOL_TAG 'ICHV'
#endif

int32_t init_vhci_driver() {
#ifdef _WIN32
    // Allocate Non-Paged Pool memory for the Ring Buffers
    g_SharedMemory = (SharedMemoryContext*)ExAllocatePoolWithTag(NonPagedPoolNx, sizeof(SharedMemoryContext), VHCI_POOL_TAG);

    if (g_SharedMemory) {
        // Must explicitly construct because ExAllocatePool just allocates raw memory
        RtlZeroMemory(g_SharedMemory, sizeof(SharedMemoryContext));
        new (&g_SharedMemory->tx_ring) SPSC_RingBuffer();
        new (&g_SharedMemory->rx_ring) SPSC_RingBuffer();
    }
#else
    // On Linux/Mock, standard allocation
    g_SharedMemory = new SharedMemoryContext();
#endif

    if (!g_SharedMemory) {
        return STATUS_UNSUCCESSFUL;
    }

    return STATUS_SUCCESS;
}

void cleanup_vhci_driver() {
    if (g_SharedMemory) {
#ifdef _WIN32
        ExFreePoolWithTag(g_SharedMemory, VHCI_POOL_TAG);
#else
        delete g_SharedMemory;
#endif
        g_SharedMemory = nullptr;
    }
}

size_t tx_ring_pop_some(uint8_t* dst, size_t max_len) {
    if (!g_SharedMemory) return 0;
    return g_SharedMemory->tx_ring.pop_some(dst, max_len);
}

bool rx_ring_push(const uint8_t* src, size_t len) {
    if (!g_SharedMemory) return false;
    return g_SharedMemory->rx_ring.push(src, len);
}

bool intercept_urb(const uint8_t* urb_data, size_t length) {
    if (!g_SharedMemory) return false;

    // The kernel intercepts the URB and writes it to the TX Ring Buffer for the user-mode daemon
    bool success = g_SharedMemory->tx_ring.push(urb_data, length);

    if (success) {
        // In a real driver, we might signal a KEVENT here to wake up the user-mode process
    }

    return success;
}
