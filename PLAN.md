# why_usb Project Plan

## Current Status

The project is currently a local mock proof of concept, not a production USB/IP
implementation.

Completed:

- Rust workspace with `daemon`, `client`, and `protocol` crates.
- C++ mock driver library built through CMake from the daemon build script.
- Tokio length-delimited TCP bridge between daemon and client.
- Binary protocol frame format in `protocol`.
- Message-boundary-aware C++ SPSC ring buffer for mock data-plane frames.
- Local mock loop: client request -> daemon RX ring -> mock driver pump -> daemon TX ring -> client response.
- Daemon service split into config, driver backend, and network session modules.
- Client service split into config, network session, and mock VHCI adapter modules.
- Configurable daemon bind address and client server address.
- Structured logging through `tracing` with `RUST_LOG` filters.
- Mock session lifecycle states for connect, attach, detach, and close.
- Protocol-level attach/detach request and response frames for explicit session lifecycle control.
- Windows IOCTL backend constants and Rust-side trait skeleton, gated behind `cfg(windows)`.
- Shared IOCTL ABI definitions in `driver/inc/ioctl.h` and Rust mirror structs in the daemon.
- Minimal KMDF IOCTL dispatch skeleton for session open/close, attach/detach, status, and shared-memory placeholder.
- Windows daemon backend opens the driver device with `CreateFileW` and issues `SESSION_OPEN`, `GET_STATUS`, and `SESSION_CLOSE` through `DeviceIoControl`.
- Windows daemon backend can issue `ATTACH_DEVICE` and `DETACH_DEVICE` through `DeviceIoControl` when `WHY_USB_ATTACH_DEVICE` is configured.
- Shared-memory layout ABI is defined, including mapping header, ring metadata, TX/RX ring offsets, and ring sizes.
- Windows daemon backend has a `GET_SHARED_MEMORY` + `MapViewOfFile` path, but it is not wired into the data plane yet.
- C++ mock `SharedMemoryContext` now starts with the ABI header and exposes layout/status helpers used by the KMDF IOCTL skeleton.
- Rust mapped-ring reader/writer is implemented and the Windows backend can use mapped TX/RX rings when `WHY_USB_MAP_SHARED_MEMORY=1`.
- KMDF device state now has a WDF device context, shared-resource handles, and cleanup/session-close resource teardown.
- Daemon TX worker can block on driver TX readiness; Windows backend waits on TX event handles and signals RX event handles when mapped shared memory is active.
- KMDF `GET_SHARED_MEMORY` has a first-pass section/event creation path and returns the section plus TX/RX event handles through the shared ABI.
- Protocol attach responses can now carry structured USB descriptor sets for the enumeration MVP.
- Protocol control transfers now model USB setup packets and can exercise GET_DESCRIPTOR, SET_ADDRESS, and SET_CONFIGURATION in the local mock path.
- Mock HID keyboard is the first enumeration target, including HID class descriptor and report descriptor validation.
- Mock HID keyboard can emit boot-protocol interrupt IN input reports after configuration.
- Mock HID keyboard input reports can be configured and queued with `WHY_USB_MOCK_HID_KEYS`.
- Daemon data-plane requests now pass through per-endpoint transfer queues with FIFO ordering per endpoint and round-robin dispatch across endpoints.
- Daemon session cleanup now explicitly drains endpoint transfer queues, and local attach/detach stress can be repeated through `scripts/local_mock_stress.sh`.
- Mock protocol and daemon queues now model failed, cancelled, timeout, stall, and reset transfer outcomes.
- Daemon can inject mock timeout, stall, failed, reset, cancelled, and short-packet transfer outcomes for local fault-path tests.
- Client and daemon can run checksum-validated mock bulk payloads for storage-class transfer experiments.
- Client can select a mock or Linux `vhci_hcd` backend and probe Linux VHCI readiness before real attach plumbing is implemented.
- Linux client can parse `vhci_hcd` sysfs status, discover free VHCI ports, and format kernel attach requests (`port sockfd devid speed`).
- Linux client can dry-run VHCI socket handoff planning from an established socket fd and a selected free VHCI port.
- Protocol crate now models the first USB/IP kernel-facing packets: `CMD_SUBMIT`, `RET_SUBMIT`, `CMD_UNLINK`, and `RET_UNLINK`.
- Daemon now has an opt-in `WHY_USB_DAEMON_PROTOCOL=usbip` socket loop that handles USB/IP submit/unlink packets for the mock HID keyboard enumeration path.
- `scripts/local_usbip_mock.sh` exercises the daemon USB/IP loop with binary submit/unlink packets, descriptor assertions, and HID interrupt IN reports.

Not completed yet:

- Real Windows KMDF USB virtual bus or stub driver behavior.
- IOCTL control channel between daemon and a kernel driver.
- Production-hardened shared memory handle duplication, requestor validation, and Windows WDK/Driver Verifier validation.
- Real Linux `vhci_hcd` sysfs socket handoff and kernel-driven enumeration validation.
- Real USB descriptors, endpoint state, URB cancellation, reset, suspend, or surprise removal handling.
- Bulk, interrupt, and isochronous transfer semantics beyond mock frames.
- Driver signing, installer, CLI, authentication, and production stability testing.

## Phase 1: Mock Core Correctness

- [x] Replace placeholder `protocol` crate with typed binary frames.
- [x] Add protocol encode/decode unit tests.
- [x] Preserve frame boundaries in the C++ ring buffer.
- [x] Make daemon/client use protocol frames instead of raw strings.
- [x] Add a mock driver pump to exercise RX and TX rings.
- [x] Add daemon/client integration tests once a Rust toolchain is available in the development environment.
- [x] Add C++ ring buffer unit tests for wrap-around, overflow, and many small frames.

## Phase 2: Service Architecture

- [x] Split daemon into modules: network session, driver backend, and config.
- [x] Split client into modules: network session, VHCI adapter, and config.
- [x] Add structured logging.
- [x] Add configurable bind/connect addresses.
- [x] Define mock session lifecycle: connect, attach, detach, disconnect, cleanup.
- [x] Define full protocol-level session lifecycle: attach request, attach response, detach request, detach response.
- [ ] Decide whether the first production target is Windows-to-Linux or Windows-to-Windows.

## Phase 3: Windows Driver Control Channel

- [x] Add Rust-side IOCTL constants and backend trait skeleton.
- [x] Define first-pass IOCTL payload structs for driver handshake, shared memory setup, event handles, attach/detach, and status.
- [x] Add KMDF I/O queue and IOCTL dispatch skeleton.
- [x] Implement daemon-side `CreateFileW` and basic `DeviceIoControl` calls.
- [x] Add ABI version validation and explicit IOCTL response parsing for session open/status.
- [x] Implement daemon-side attach/detach IOCTL calls.
- [x] Define shared-memory mapping header and TX/RX ring layout.
- [x] Add daemon-side `GET_SHARED_MEMORY` and `MapViewOfFile` mapping wrapper.
- [x] Align C++ mock shared-memory layout with the shared ABI header and ring metadata.
- [x] Add Rust mapped-ring data-plane reader/writer.
- [x] Wire Windows backend TX/RX methods to mapped rings when shared memory is mapped.
- [x] Add KMDF device context and shared-resource cleanup lifecycle.
- [x] Add daemon-side event wait/signal hooks for mapped TX/RX rings.
- [x] Replace the current user-mode `new SharedMemoryContext()` mock with a first-pass KMDF section mapping when `GET_SHARED_MEMORY` is requested.
- [x] Create first-pass section/event handles for `GET_SHARED_MEMORY` instead of returning `STATUS_NOT_IMPLEMENTED`.
- [ ] Harden `GET_SHARED_MEMORY` by duplicating handles into the daemon process and validating the IOCTL requestor.
- [ ] Add KMDF-side event signaling when TX/RX ring state changes.
- [ ] Run Driver Verifier against load/unload and IOCTL open/close paths.

## Phase 4: USB Enumeration MVP

- [ ] Implement or integrate a virtual USB bus path.
- [x] Represent remote device descriptors in the protocol.
- [x] Support control transfers required for enumeration.
- [x] Fix mock HID keyboard as the first local enumeration target.
- [x] Add Linux `vhci_hcd` backend selection and readiness probe in the client.
- [x] Parse Linux `vhci_hcd` sysfs status and model attach requests.
- [x] Add Linux VHCI socket handoff dry-run planning.
- [x] Add USB/IP submit/unlink packet encoding and decoding.
- [x] Add daemon-side USB/IP socket loop for mock control endpoint and HID interrupt IN reports.
- [ ] Validate that a remote device can appear in the client OS with correct descriptors.
- [ ] Target first real device class: HID mouse or keyboard.

## Phase 5: Transfer Semantics

- [x] Add mock HID interrupt IN input report model.
- [x] Add configurable mock HID input report queue.
- [ ] Implement real interrupt transfers for HID through the OS virtual USB path.
- [x] Add checksum-validated mock bulk transfer workload for storage-class experiments.
- [ ] Implement real bulk transfers through the OS virtual USB path.
- [x] Add mock protocol/status model for request cancellation, timeout, stall, and reset.
- [x] Add daemon queue cancellation and endpoint reset cleanup semantics.
- [x] Add local mock fault injection for timeout, stall, and short packet transfer responses.
- [ ] Implement request cancellation, timeout, short packet, stall, and reset against the real OS virtual USB path.
- [x] Track per-endpoint queues and transfer ordering in the local daemon model.
- [x] Add stress tests for disconnects and repeated attach/detach.

## Phase 6: Performance Work

- [ ] Batch small frames where safe.
- [ ] Tune socket buffer sizes and TCP settings.
- [ ] Reduce copies across ring buffer and network boundaries.
- [ ] Measure throughput with storage devices.
- [ ] Add isochronous transfer support with late-frame drop policy for webcam/audio.

## Phase 7: Productization

- [ ] Add CLI commands: `list`, `share`, `attach`, `detach`, and `status`.
- [ ] Add authorization/authentication for remote clients.
- [ ] Provide Windows install/uninstall scripts.
- [ ] Document WDK setup, signing requirements, debugging, and crash dump collection.
- [ ] Produce repeatable release builds.
