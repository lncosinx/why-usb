#include "device.h"
#include "vhci.h"

NTSTATUS
why_usb_EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    UNREFERENCED_PARAMETER(Driver);
    return why_usb_CreateDevice(DeviceInit);
}

NTSTATUS
why_usb_CreateDevice(
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE device;
    NTSTATUS status;
    WDF_IO_QUEUE_CONFIG queueConfig;

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, DEVICE_CONTEXT);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &device);

    if (NT_SUCCESS(status)) {
        PDEVICE_CONTEXT deviceContext = DeviceGetContext(device);
        deviceContext->WdfDevice = device;

        // Initialize default I/O queue to receive URBs via DeviceIoControl
        WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
        queueConfig.EvtIoDeviceControl = why_usb_EvtIoDeviceControl;

        status = WdfIoQueueCreate(device,
                                  &queueConfig,
                                  WDF_NO_OBJECT_ATTRIBUTES,
                                  WDF_NO_HANDLE);
    }

    return status;
}

VOID
why_usb_EvtIoDeviceControl(
    _In_ WDFQUEUE Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t OutputBufferLength,
    _In_ size_t InputBufferLength,
    _In_ ULONG IoControlCode
)
{
    UNREFERENCED_PARAMETER(Queue);
    UNREFERENCED_PARAMETER(OutputBufferLength);

    NTSTATUS status = STATUS_SUCCESS;
    PVOID buffer = NULL;
    size_t length = 0;

    // A real driver would process specific URB IOCTLs here (e.g., IOCTL_INTERNAL_USB_SUBMIT_URB)
    // For now, we simulate intercepting a payload and pushing it to our TX Ring Buffer.

    if (InputBufferLength > 0) {
        status = WdfRequestRetrieveInputBuffer(Request, InputBufferLength, &buffer, &length);
        if (NT_SUCCESS(status) && buffer != NULL) {
            bool pushed = intercept_urb((const uint8_t*)buffer, length);
            if (!pushed) {
                status = STATUS_UNSUCCESSFUL; // Ring Buffer Full
            }
        }
    }

    WdfRequestComplete(Request, status);
}
