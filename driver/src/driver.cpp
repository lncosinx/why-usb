#include "driver.h"
#include "device.h"
#include "vhci.h"

extern "C" NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    NTSTATUS status;
    WDF_DRIVER_CONFIG config;
    WDF_OBJECT_ATTRIBUTES attributes;

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "why_usb: DriverEntry begins\n"));

    WDF_DRIVER_CONFIG_INIT(&config, why_usb_EvtDeviceAdd);

    WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
    attributes.EvtCleanupCallback = why_usb_EvtDriverContextCleanup;

    status = WdfDriverCreate(DriverObject,
                             RegistryPath,
                             &attributes,
                             &config,
                             WDF_NO_HANDLE);

    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL, "why_usb: WdfDriverCreate failed %!STATUS!\n", status));
        return status;
    }

    // Initialize our Ring Buffer and Shared Memory Context
    status = init_vhci_driver();
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL, "why_usb: init_vhci_driver failed\n"));
        return status;
    }

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "why_usb: DriverEntry ends successfully\n"));
    return status;
}

VOID
why_usb_EvtDriverContextCleanup(
    _In_ WDFOBJECT DriverObject
)
{
    UNREFERENCED_PARAMETER(DriverObject);
    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "why_usb: Cleanup Context\n"));

    // Clean up Shared Memory pool
    cleanup_vhci_driver();
}
