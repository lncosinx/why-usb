#pragma once

#include <ntddk.h>
#include <wdf.h>

typedef struct _DEVICE_CONTEXT {
    WDFDEVICE WdfDevice;
} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, DeviceGetContext)

NTSTATUS
why_usb_CreateDevice(
    _Inout_ PWDFDEVICE_INIT DeviceInit
);

EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL why_usb_EvtIoDeviceControl;
