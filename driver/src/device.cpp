#include <ntddk.h>
#include <wdf.h>

#include "ioctl.h"
#include "ring_buffer.h"
#include "vhci.h"

static uint64_t g_SessionId = 1;
static WHY_USB_SESSION_STATE g_SessionState = WHY_USB_SESSION_CLOSED;

typedef struct _WHY_USB_DEVICE_CONTEXT {
    HANDLE SectionHandle;
    HANDLE TxEventHandle;
    HANDLE RxEventHandle;
    PKEVENT TxEventObject;
    PKEVENT RxEventObject;
    PVOID SectionView;
    SIZE_T SectionViewSize;
    BOOLEAN SharedMemoryReady;
    WhyUsbSharedMemoryInfo SharedMemoryInfo;
} WHY_USB_DEVICE_CONTEXT, *PWHY_USB_DEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(WHY_USB_DEVICE_CONTEXT, WhyUsbGetDeviceContext)

extern "C" VOID
EvtDeviceContextCleanup(
    _In_ WDFOBJECT DeviceObject
);

extern "C" VOID
EvtIoDeviceControl(
    _In_ WDFQUEUE Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t OutputBufferLength,
    _In_ size_t InputBufferLength,
    _In_ ULONG IoControlCode
);

static void CloseIfPresent(HANDLE* Handle)
{
    if (Handle && *Handle) {
        ZwClose(*Handle);
        *Handle = nullptr;
    }
}

static void PopulateAbiHeader(WhyUsbAbiHeader* Header, uint16_t Size)
{
    Header->magic = WHY_USB_ABI_MAGIC;
    Header->version = WHY_USB_ABI_VERSION;
    Header->size = Size;
}

static uint64_t HandleToU64(HANDLE Handle)
{
    return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(Handle));
}

static NTSTATUS EnsureDriverMemoryContext()
{
    WhyUsbStatusResponse status = {};
    if (get_driver_status(&status)) {
        return STATUS_SUCCESS;
    }

    return init_vhci_driver();
}

static void DerefEventIfPresent(PKEVENT* EventObject)
{
    if (EventObject && *EventObject) {
        ObDereferenceObject(*EventObject);
        *EventObject = nullptr;
    }
}

static void CloseSharedResources(PWHY_USB_DEVICE_CONTEXT Context)
{
    if (!Context) {
        return;
    }

    if (Context->SectionView) {
        ZwUnmapViewOfSection(NtCurrentProcess(), Context->SectionView);
        Context->SectionView = nullptr;
        Context->SectionViewSize = 0;
    }

    use_external_shared_memory_context(nullptr);

    DerefEventIfPresent(&Context->TxEventObject);
    DerefEventIfPresent(&Context->RxEventObject);

    CloseIfPresent(&Context->SectionHandle);
    CloseIfPresent(&Context->TxEventHandle);
    CloseIfPresent(&Context->RxEventHandle);
    Context->SharedMemoryReady = FALSE;
    RtlZeroMemory(&Context->SharedMemoryInfo, sizeof(Context->SharedMemoryInfo));
}

static void InitializeSharedMemoryContext(SharedMemoryContext* Context)
{
    RtlZeroMemory(Context, sizeof(*Context));
    Context->header.magic = WHY_USB_SHARED_MEMORY_MAGIC;
    Context->header.version = WHY_USB_SHARED_MEMORY_VERSION;
    Context->header.header_size = sizeof(WhyUsbSharedMemoryHeader);
    Context->header.mapping_size = sizeof(SharedMemoryContext);
    Context->header.tx_ring_offset = offsetof(SharedMemoryContext, tx_ring);
    Context->header.rx_ring_offset = offsetof(SharedMemoryContext, rx_ring);
    Context->header.tx_ring_size = sizeof(SPSC_RingBuffer);
    Context->header.rx_ring_size = sizeof(SPSC_RingBuffer);
    Context->tx_ring.head.store(0, std::memory_order_relaxed);
    Context->tx_ring.tail.store(0, std::memory_order_relaxed);
    Context->rx_ring.head.store(0, std::memory_order_relaxed);
    Context->rx_ring.tail.store(0, std::memory_order_relaxed);
}

static NTSTATUS CreateSharedResources(PWHY_USB_DEVICE_CONTEXT Context)
{
    LARGE_INTEGER maximumSize = {};
    maximumSize.QuadPart = sizeof(SharedMemoryContext);

    OBJECT_ATTRIBUTES attributes;
    InitializeObjectAttributes(&attributes, nullptr, OBJ_KERNEL_HANDLE, nullptr, nullptr);

    NTSTATUS status = ZwCreateSection(
        &Context->SectionHandle,
        SECTION_MAP_READ | SECTION_MAP_WRITE,
        &attributes,
        &maximumSize,
        PAGE_READWRITE,
        SEC_COMMIT,
        nullptr
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    Context->SectionView = nullptr;
    Context->SectionViewSize = 0;

    status = ZwMapViewOfSection(
        Context->SectionHandle,
        NtCurrentProcess(),
        &Context->SectionView,
        0,
        sizeof(SharedMemoryContext),
        nullptr,
        &Context->SectionViewSize,
        ViewUnmap,
        0,
        PAGE_READWRITE
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    InitializeSharedMemoryContext(reinterpret_cast<SharedMemoryContext*>(Context->SectionView));
    use_external_shared_memory_context(reinterpret_cast<SharedMemoryContext*>(Context->SectionView));

    // Ensure event handles are also created with OBJ_KERNEL_HANDLE
    OBJECT_ATTRIBUTES eventAttributes;
    InitializeObjectAttributes(&eventAttributes, nullptr, OBJ_KERNEL_HANDLE, nullptr, nullptr);

    status = ZwCreateEvent(
        &Context->TxEventHandle,
        EVENT_MODIFY_STATE | SYNCHRONIZE,
        &eventAttributes,
        SynchronizationEvent,
        FALSE
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = ObReferenceObjectByHandle(
        Context->TxEventHandle,
        EVENT_MODIFY_STATE,
        ExEventObjectType,
        KernelMode,
        reinterpret_cast<PVOID*>(&Context->TxEventObject),
        nullptr
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = ZwCreateEvent(
        &Context->RxEventHandle,
        EVENT_MODIFY_STATE | SYNCHRONIZE,
        &eventAttributes,
        SynchronizationEvent,
        FALSE
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = ObReferenceObjectByHandle(
        Context->RxEventHandle,
        EVENT_MODIFY_STATE,
        ExEventObjectType,
        KernelMode,
        reinterpret_cast<PVOID*>(&Context->RxEventObject),
        nullptr
    );

    return status;
}

static NTSTATUS EnsureSharedResources(PWHY_USB_DEVICE_CONTEXT Context)
{
    if (!Context) {
        return STATUS_INVALID_PARAMETER;
    }

    if (Context->SharedMemoryReady) {
        return STATUS_SUCCESS;
    }

    NTSTATUS status = CreateSharedResources(Context);
    if (!NT_SUCCESS(status)) {
        CloseSharedResources(Context);
        return status;
    }

    if (!get_shared_memory_info(&Context->SharedMemoryInfo)) {
        CloseSharedResources(Context);
        return STATUS_INVALID_DEVICE_STATE;
    }

    Context->SharedMemoryReady = TRUE;

    return STATUS_SUCCESS;
}

static NTSTATUS DuplicateHandleToRequestor(WDFREQUEST Request, HANDLE KernelHandle, PHANDLE UserHandle)
{
    PIRP irp = WdfRequestWdmGetIrp(Request);
    PEPROCESS requestorProcess = IoGetRequestorProcess(irp);
    if (!requestorProcess) {
        return STATUS_INVALID_PARAMETER;
    }

    HANDLE requestorProcessHandle = nullptr;
    NTSTATUS status = ObOpenObjectByPointer(
        requestorProcess,
        OBJ_KERNEL_HANDLE,
        nullptr,
        0,
        PsProcessType,
        KernelMode,
        &requestorProcessHandle
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = ZwDuplicateObject(
        NtCurrentProcess(),
        KernelHandle,
        requestorProcessHandle,
        UserHandle,
        0,
        0,
        DUPLICATE_SAME_ACCESS
    );

    ZwClose(requestorProcessHandle);
    return status;
}

template <typename T>
static NTSTATUS CompleteWithStruct(WDFREQUEST Request, const T& value)
{
    void* outputBuffer = nullptr;
    NTSTATUS status = WdfRequestRetrieveOutputBuffer(Request, sizeof(T), &outputBuffer, nullptr);

    if (!NT_SUCCESS(status)) {
        WdfRequestCompleteWithInformation(Request, status, 0);
        return status;
    }

    RtlCopyMemory(outputBuffer, &value, sizeof(T));
    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, sizeof(T));
    return STATUS_SUCCESS;
}

static PWHY_USB_DEVICE_CONTEXT g_GlobalDeviceContext = nullptr;

static void GlobalTxEventSignalHook()
{
    if (g_GlobalDeviceContext && g_GlobalDeviceContext->TxEventObject) {
        KeSetEvent(g_GlobalDeviceContext->TxEventObject, 0, FALSE);
    }
}

static void GlobalRxEventSignalHook()
{
    if (g_GlobalDeviceContext && g_GlobalDeviceContext->RxEventObject) {
        KeSetEvent(g_GlobalDeviceContext->RxEventObject, 0, FALSE);
    }
}

extern "C" NTSTATUS
EvtDriverDeviceAdd(
    _In_ WDFDRIVER Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    NTSTATUS status;
    WDFDEVICE device;
    WDFQUEUE queue;
    WDF_IO_QUEUE_CONFIG queueConfig;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;

    UNREFERENCED_PARAMETER(Driver);

    KdPrint(("why_usb_vhci: EvtDriverDeviceAdd\n"));

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, WHY_USB_DEVICE_CONTEXT);
    deviceAttributes.EvtCleanupCallback = EvtDeviceContextCleanup;

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &device);

    if (!NT_SUCCESS(status)) {
        KdPrint(("why_usb_vhci: WdfDeviceCreate failed with status 0x%x\n", status));
        return status;
    }

    g_GlobalDeviceContext = WhyUsbGetDeviceContext(device);
    set_tx_event_signal_hook(GlobalTxEventSignalHook);
    set_rx_event_signal_hook(GlobalRxEventSignalHook);

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchSequential);
    queueConfig.EvtIoDeviceControl = EvtIoDeviceControl;

    status = WdfIoQueueCreate(
        device,
        &queueConfig,
        WDF_NO_OBJECT_ATTRIBUTES,
        &queue
    );

    if (!NT_SUCCESS(status)) {
        KdPrint(("why_usb_vhci: WdfIoQueueCreate failed with status 0x%x\n", status));
        return status;
    }

    return status;
}

extern "C" VOID
EvtDeviceContextCleanup(
    _In_ WDFOBJECT DeviceObject
)
{
    auto context = WhyUsbGetDeviceContext(DeviceObject);
    if (g_GlobalDeviceContext == context) {
        set_tx_event_signal_hook(nullptr);
        set_rx_event_signal_hook(nullptr);
        g_GlobalDeviceContext = nullptr;
    }
    CloseSharedResources(context);
}

extern "C" VOID
EvtIoDeviceControl(
    _In_ WDFQUEUE Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t OutputBufferLength,
    _In_ size_t InputBufferLength,
    _In_ ULONG IoControlCode
)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    auto context = WhyUsbGetDeviceContext(device);

    switch (IoControlCode) {
    case IOCTL_WHY_USB_SESSION_OPEN: {
        NTSTATUS status = EnsureDriverMemoryContext();
        if (!NT_SUCCESS(status)) {
            WdfRequestCompleteWithInformation(Request, status, 0);
            break;
        }

        g_SessionState = WHY_USB_SESSION_OPEN;
        WhyUsbSessionOpenResponse response = {};
        PopulateAbiHeader(&response.header, sizeof(response));
        response.session_id = g_SessionId;
        response.status = WHY_USB_STATUS_OK;
        response.max_frame_size = 64 * 1024;
        CompleteWithStruct(Request, response);
        break;
    }

    case IOCTL_WHY_USB_SESSION_CLOSE:
        g_SessionState = WHY_USB_SESSION_CLOSED;
        CloseSharedResources(context);
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        break;

    case IOCTL_WHY_USB_GET_SHARED_MEMORY: {
        NTSTATUS status = EnsureSharedResources(context);
        if (!NT_SUCCESS(status)) {
            WdfRequestCompleteWithInformation(Request, status, 0);
            break;
        }

        HANDLE userSectionHandle = nullptr;
        HANDLE userTxEventHandle = nullptr;
        HANDLE userRxEventHandle = nullptr;

        status = DuplicateHandleToRequestor(Request, context->SectionHandle, &userSectionHandle);
        if (NT_SUCCESS(status)) {
            status = DuplicateHandleToRequestor(Request, context->TxEventHandle, &userTxEventHandle);
        }
        if (NT_SUCCESS(status)) {
            status = DuplicateHandleToRequestor(Request, context->RxEventHandle, &userRxEventHandle);
        }

        if (!NT_SUCCESS(status)) {
            // Note: If we fail here, userSectionHandle/userTxEventHandle might already be
            // valid in the *requestor's* handle table. We cannot just call ZwClose() here
            // because we might be in an arbitrary system thread context, which would close
            // a random handle in the system process!
            // To properly clean up handles duplicated into another process, we would need to
            // use KeStackAttachProcess or duplicate them back with DUPLICATE_CLOSE_SOURCE.
            // For now, in this error path, we simply let the user process clean them up on exit.
            WdfRequestCompleteWithInformation(Request, status, 0);
            break;
        }

        context->SharedMemoryInfo.section_handle = HandleToU64(userSectionHandle);
        context->SharedMemoryInfo.tx_event_handle = HandleToU64(userTxEventHandle);
        context->SharedMemoryInfo.rx_event_handle = HandleToU64(userRxEventHandle);

        CompleteWithStruct(Request, context->SharedMemoryInfo);
        break;
    }

    case IOCTL_WHY_USB_ATTACH_DEVICE:
        g_SessionState = WHY_USB_SESSION_ATTACHED;
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        break;

    case IOCTL_WHY_USB_DETACH_DEVICE:
        g_SessionState = WHY_USB_SESSION_OPEN;
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        break;

    case IOCTL_WHY_USB_GET_STATUS: {
        WhyUsbStatusResponse response = {};
        if (!get_driver_status(&response)) {
            WdfRequestCompleteWithInformation(Request, STATUS_INVALID_DEVICE_STATE, 0);
            break;
        }
        response.session_id = g_SessionId;
        response.session_state = g_SessionState;
        if (!context->SharedMemoryReady && g_SessionState != WHY_USB_SESSION_CLOSED) {
            response.status = WHY_USB_STATUS_UNSUPPORTED;
        }
        CompleteWithStruct(Request, response);
        break;
    }

    default:
        WdfRequestCompleteWithInformation(Request, STATUS_INVALID_DEVICE_REQUEST, 0);
        break;
    }
}
