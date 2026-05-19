# why_usb Project Plan

## Phase 1: Engineering Scaffold & FFI Bridge
- [x] Initialize directory structure (`driver/`, `daemon/`, `client/`, `protocol/`)
- [x] Configure Rust workspace and Cargo.toml
- [x] Configure C++ mock driver with CMake
- [x] Setup `cxx` bridge in `daemon` and verify FFI execution

## Phase 2: Windows Kernel Driver Development (C++ / WDF)
- [x] Setup KMDF driver framework
- [x] Intercept URB requests
- [x] Implement Shared Memory Ring Buffer (TX/RX)
- [x] Create `driver/src/driver.cpp` with standard KMDF driver initialization boilerplate.
- [x] Create `driver/src/device.cpp` with standard KMDF device initialization boilerplate.
- [x] Create `driver/why_usb_vhci.inf` with standard driver configuration boilerplate.
- [x] Update `driver/CMakeLists.txt` to conditionally compile `driver.cpp` and `device.cpp` only when building for Windows.

## Phase 3: User-mode Daemon Development (Rust / Tokio)
- [x] Implement memory mapping from driver in Rust
- [x] Create Toki-based async TCP Server
- [x] Implement fast frame decoder/encoder for URBs

## Phase 4: Linux/WSL2 Client Development (Rust)
- [x] Implement Linux vhci-hcd protocol adapter
- [x] Implement Toki-based network client to connect to Server
- [x] Add bytemuck dependency to protocol
- [x] Define URB raw binary structs in protocol crate
- [x] Construct and serialize protocol URB structs in client

## Phase 5: Verification & Testing
- [x] Basic connectivity test (e.g., USB Mouse)
- [x] High throughput test (e.g., USB 3.0 Flash drive)
- [x] High frequency concurrency test (e.g., Webcam)
- [x] Memory leak & BSOD stability verification
- [x] Verify file creations and CMake configuration changes.
- [x] Run integration tests to ensure project still builds on Linux.
