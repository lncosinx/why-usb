#include "vhci.h"
#include "ring_buffer.h"

// Do not include standard libraries like iostream or memory
// as they are not available in the Windows kernel mode context.

// Global context pointer to simulate driver extension
static SharedMemoryContext* g_SharedMemory = nullptr;

#ifdef _WIN32
// We provide dummy declarations here just to make the Linux mock build pass
// In a real Windows build, these would be provided by WDF headers
extern "C" {
    NTSTATUS DriverEntry(void* DriverObject, void* RegistryPath) {
        // Initialize WDF driver object here
        return init_vhci_driver();
    }
}
#endif

int32_t init_vhci_driver() {
#ifdef _WIN32
    // On real Windows, this would be allocated via ExAllocatePoolWithTag
    // g_SharedMemory = (SharedMemoryContext*)ExAllocatePoolWithTag(NonPagedPool, sizeof(SharedMemoryContext), 'VHCI');
    // For now, even on Windows, if WDF isn't fully linked, we fallback to new.
    g_SharedMemory = new SharedMemoryContext();
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
        // ExFreePool(g_SharedMemory);
        delete g_SharedMemory;
#else
        delete g_SharedMemory;
#endif
        g_SharedMemory = nullptr;
    }
}

SharedMemoryContext* get_shared_memory() {
    return g_SharedMemory;
}

// Mock of what the EvtIoInternalDeviceControl callback would do when it receives an URB
bool intercept_urb(const uint8_t* urb_data, size_t length) {
    if (!g_SharedMemory) return false;

    // The kernel intercepts the URB and writes it to the TX Ring Buffer for the user-mode daemon
    bool success = g_SharedMemory->tx_ring.push(urb_data, length);

    if (success) {
        // In a real driver, we might signal a KEVENT here to wake up the user-mode process
    }

    return success;
}
