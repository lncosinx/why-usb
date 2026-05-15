#pragma once

#include <ntddk.h>
#include <wdf.h>

// Forward declarations
extern "C" DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD why_usb_EvtDeviceAdd;
EVT_WDF_OBJECT_CONTEXT_CLEANUP why_usb_EvtDriverContextCleanup;
