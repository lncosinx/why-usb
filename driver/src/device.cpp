#include <ntddk.h>
#include <wdf.h>

extern "C" NTSTATUS
EvtDriverDeviceAdd(
    _In_ WDFDRIVER Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    NTSTATUS status;
    WDFDEVICE device;

    UNREFERENCED_PARAMETER(Driver);

    KdPrint(("why_usb_vhci: EvtDriverDeviceAdd\n"));

    status = WdfDeviceCreate(&DeviceInit, WDF_NO_OBJECT_ATTRIBUTES, &device);

    if (!NT_SUCCESS(status)) {
        KdPrint(("why_usb_vhci: WdfDeviceCreate failed with status 0x%x\n", status));
        return status;
    }

    return status;
}
