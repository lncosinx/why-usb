#include "vhci.h"
#include "ring_buffer.h"

#ifdef _WIN32
#include <wdm.h>
#endif

// Do not include standard libraries like iostream or memory
// as they are not available in the Windows kernel mode context.

// Global context pointer to simulate driver extension
static SharedMemoryContext* g_SharedMemory = nullptr;
static bool g_OwnsSharedMemory = false;

static void* g_TxEvent = nullptr;
static void* g_RxEvent = nullptr;

void set_vhci_events(void* tx_event, void* rx_event) {
    g_TxEvent = tx_event;
    g_RxEvent = rx_event;
}

static void release_owned_shared_memory() {
    if (g_SharedMemory && g_OwnsSharedMemory) {
        delete g_SharedMemory;
    }

    g_SharedMemory = nullptr;
    g_OwnsSharedMemory = false;
}

int32_t init_vhci_driver() {
    release_owned_shared_memory();

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

    g_OwnsSharedMemory = true;
    return STATUS_SUCCESS;
}

void cleanup_vhci_driver() {
    release_owned_shared_memory();
}

void use_external_shared_memory_context(SharedMemoryContext* context) {
    release_owned_shared_memory();
    g_SharedMemory = context;
    g_OwnsSharedMemory = false;
}

bool tx_ring_pop_frame(uint8_t* dst, size_t max_len, size_t* out_len) {
    if (!g_SharedMemory) return false;
    return g_SharedMemory->tx_ring.pop_frame(dst, max_len, out_len);
}

bool rx_ring_push_frame(const uint8_t* src, size_t len) {
    if (!g_SharedMemory) return false;
    return g_SharedMemory->rx_ring.push_frame(src, len);
}

bool get_shared_memory_info(WhyUsbSharedMemoryInfo* info) {
    if (!g_SharedMemory || !info) return false;

    info->header.magic = WHY_USB_ABI_MAGIC;
    info->header.version = WHY_USB_ABI_VERSION;
    info->header.size = sizeof(WhyUsbSharedMemoryInfo);
    info->section_handle = 0;
    info->tx_event_handle = 0;
    info->rx_event_handle = 0;
    info->mapping_size = g_SharedMemory->header.mapping_size;
    info->tx_ring_size = g_SharedMemory->header.tx_ring_size;
    info->rx_ring_size = g_SharedMemory->header.rx_ring_size;
    info->tx_ring_offset = g_SharedMemory->header.tx_ring_offset;
    info->rx_ring_offset = g_SharedMemory->header.rx_ring_offset;
    return true;
}

bool get_driver_status(WhyUsbStatusResponse* status) {
    if (!g_SharedMemory || !status) return false;

    status->header.magic = WHY_USB_ABI_MAGIC;
    status->header.version = WHY_USB_ABI_VERSION;
    status->header.size = sizeof(WhyUsbStatusResponse);
    status->session_id = 1;
    status->status = WHY_USB_STATUS_OK;
    status->session_state = WHY_USB_SESSION_OPEN;
    status->tx_queued_bytes = static_cast<uint32_t>(g_SharedMemory->tx_ring.available_data());
    status->rx_queued_bytes = static_cast<uint32_t>(g_SharedMemory->rx_ring.available_data());
    return true;
}

// Mock of what the EvtIoInternalDeviceControl callback would do when it receives an URB
bool intercept_urb(const uint8_t* urb_data, size_t length) {
    if (!g_SharedMemory) return false;

    // The kernel intercepts the URB and writes it to the TX Ring Buffer for the user-mode daemon
    bool success = g_SharedMemory->tx_ring.push_frame(urb_data, length);

    if (success) {
#ifdef _WIN32
        if (g_TxEvent) {
            KeSetEvent((PKEVENT)g_TxEvent, 0, FALSE);
        }
#endif
    }

    return success;
}

bool mock_driver_pump_once() {
    if (!g_SharedMemory) return false;

    uint8_t buffer[64 * 1024];
    size_t frame_len = 0;

    if (!g_SharedMemory->rx_ring.pop_frame(buffer, sizeof(buffer), &frame_len)) {
        return false;
    }

    bool success = g_SharedMemory->tx_ring.push_frame(buffer, frame_len);

    if (success) {
#ifdef _WIN32
        if (g_TxEvent) {
            KeSetEvent((PKEVENT)g_TxEvent, 0, FALSE);
        }
#endif
    }

    return success;
}
