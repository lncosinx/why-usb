#include <ntddk.h>
#include <wdf.h>

#include "vhci.h"

// Forward declaration of EvtDriverDeviceAdd
extern "C" NTSTATUS
EvtDriverDeviceAdd(
    _In_ WDFDRIVER Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
);

extern "C" VOID
EvtDriverUnload(
    _In_ WDFDRIVER Driver
);

extern "C" NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    NTSTATUS status;
    WDF_DRIVER_CONFIG config;

    KdPrint(("why_usb_vhci: DriverEntry Build Date %s %s\n", __DATE__, __TIME__));

    WDF_DRIVER_CONFIG_INIT(&config, EvtDriverDeviceAdd);
    config.EvtDriverUnload = EvtDriverUnload;

    status = WdfDriverCreate(
        DriverObject,
        RegistryPath,
        WDF_NO_OBJECT_ATTRIBUTES,
        &config,
        WDF_NO_HANDLE
    );

    if (!NT_SUCCESS(status)) {
        KdPrint(("why_usb_vhci: WdfDriverCreate failed with status 0x%x\n", status));
        return status;
    }

    // Call our shared memory initialization
    status = init_vhci_driver();
    if (!NT_SUCCESS(status)) {
         KdPrint(("why_usb_vhci: init_vhci_driver failed with status 0x%x\n", status));
         return status;
    }

    return status;
}

extern "C" VOID
EvtDriverUnload(
    _In_ WDFDRIVER Driver
)
{
    UNREFERENCED_PARAMETER(Driver);
    KdPrint(("why_usb_vhci: EvtDriverUnload\n"));

    // Cleanup shared memory
    cleanup_vhci_driver();
}
