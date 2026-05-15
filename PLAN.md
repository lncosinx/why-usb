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

## Phase 3: User-mode Daemon Development (Rust / Tokio)
- [ ] Implement memory mapping from driver in Rust
- [ ] Create Toki-based async TCP Server
- [ ] Implement fast frame decoder/encoder for URBs

## Phase 4: Linux/WSL2 Client Development (Rust)
- [ ] Implement Linux vhci-hcd protocol adapter
- [ ] Implement Toki-based network client to connect to Server

## Phase 5: Verification & Testing
- [ ] Basic connectivity test (e.g., USB Mouse)
- [ ] High throughput test (e.g., USB 3.0 Flash drive)
- [ ] High frequency concurrency test (e.g., Webcam)
- [ ] Memory leak & BSOD stability verification
