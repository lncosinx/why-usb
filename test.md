# why_usb: 测试与验证流程指南

本文档详细描述了 `why_usb` 项目的测试流程、步骤、方法以及预期行为。测试分为两个阶段：**本地 Mock 联调测试** (目前可直接在任意 Linux/WSL 环境中运行) 和 **生产环境极限测试** (需要在部署了真实 WDF 驱动的 Windows Host 和 Linux Client 之间进行)。

这对应了 `agent.md` 和 `PLAN.md` 中的 Phase 5 阶段目标。

---

## 阶段一：本地 Mock 联调测试 (Local Integration Test)

这一阶段的目的是验证 C++ 驱动架构、共享内存 Ring Buffer 以及 Rust Tokio 网络层的基本正确性。因为我们目前在 Linux 下通过条件编译 (`#ifdef _WIN32` 的 `#else` 分支) 模拟了 WDF 环境。

### 1. 测试步骤

1. **编译整个工作空间**:
   ```bash
   cargo build
   ```
   C++ mock driver 和 ring buffer 单元测试也可以单独验证：
   ```bash
   cmake -S driver -B /tmp/why-usb-driver-build
   cmake --build /tmp/why-usb-driver-build
   ctest --test-dir /tmp/why-usb-driver-build --output-on-failure
   ```
   如果本机 CMake 版本较老，`ctest --test-dir` 可能不会切换目录；可改用：
   ```bash
   cd /tmp/why-usb-driver-build
   ctest --output-on-failure
   ```
   daemon/client 本地 mock 联调可以自动化运行：
   ```bash
   bash scripts/local_mock_integration.sh
   ```
   也可以重复执行完整 attach/enumerate/HID/data/detach 流程，验证断开和重复 attach/detach 路径不会留下 endpoint transfer queue 残留：
   ```bash
   WHY_USB_STRESS_ITERATIONS=10 bash scripts/local_mock_stress.sh
   ```
   也可以注入 mock transfer fault，验证 timeout、stall、short packet 和 detach cleanup：
   ```bash
   bash scripts/local_mock_faults.sh
   ```
   bulk payload 本地 mock workload 可以用来验证 storage-class 实验前的 payload 大小、checksum 和返回路径：
   ```bash
   bash scripts/local_mock_bulk.sh
   ```
   如需避免端口冲突，可覆盖监听地址：
   ```bash
   WHY_USB_INTEGRATION_ADDR=127.0.0.1:3020 bash scripts/local_mock_integration.sh
   ```
2. **启动 Server (Daemon)**:
   在终端 1 中运行守护进程：
   ```bash
   ./target/debug/daemon
   ```
   默认监听 `0.0.0.0:3000`。也可以使用命令行参数或环境变量覆盖：
   ```bash
   ./target/debug/daemon 127.0.0.1:3001
   WHY_USB_BIND_ADDR=127.0.0.1:3001 ./target/debug/daemon
   ```
   日志使用 `tracing`，默认是 `info`。可以用 `RUST_LOG` 调整：
   ```bash
   RUST_LOG=debug ./target/debug/daemon
   ```
   默认使用 mock driver backend。Windows 上后续可以通过环境变量选择 IOCTL backend：
   ```bash
   WHY_USB_DRIVER_BACKEND=windows WHY_USB_DRIVER_DEVICE='\\.\why_usb_vhci' ./target/debug/daemon
   ```
   如需触发 attach/detach IOCTL，可设置目标设备选择器：
   ```bash
   WHY_USB_DRIVER_BACKEND=windows \
   WHY_USB_MAP_SHARED_MEMORY=1 \
   WHY_USB_ATTACH_DEVICE=1234:5678:1:2 \
   ./target/debug/daemon
   ```
   `WHY_USB_ATTACH_DEVICE` 格式为 `vid:pid[:bus[:port]]`，数字默认按十六进制解析，也接受十进制 fallback。`WHY_USB_MOCK_HID_KEYS` 可配置 mock HID keyboard 的输入报告队列，例如 `a,enter,0x2c`。当前 Windows backend 已实现基础 `CreateFileW` / `DeviceIoControl` 路径，会执行 `SESSION_OPEN`、`GET_STATUS`、可选 `ATTACH_DEVICE`、可选 `DETACH_DEVICE`，退出时执行 `SESSION_CLOSE`。设置 `WHY_USB_MAP_SHARED_MEMORY=1` 后会尝试 `GET_SHARED_MEMORY` 和 `MapViewOfFile`，并使用映射 TX/RX ring 数据面；如果驱动返回 event handles，daemon 会等待 TX event 并在写 RX ring 后 signal RX event。KMDF 侧已建立 device context、资源清理生命周期，以及 first-pass section/event 创建路径。该路径仍需在真实 Windows WDK 环境中验证，并继续补上 handle duplication 与 requestor validation。
3. **启动 Client**:
   在终端 2 中运行客户端：
   ```bash
   ./target/debug/client
   ```
   默认连接 `127.0.0.1:3000`。也可以使用命令行参数或环境变量覆盖：
   ```bash
   ./target/debug/client 127.0.0.1:3001
   WHY_USB_SERVER_ADDR=127.0.0.1:3001 ./target/debug/client
   ```
   同样可以使用 `RUST_LOG` 调整客户端日志：
   ```bash
   RUST_LOG=debug ./target/debug/client
   ```
   默认 client 使用 mock VHCI backend。Linux/WSL 上可先探测真实 `vhci_hcd` 是否就绪：
   ```bash
   WHY_USB_VHCI_BACKEND=linux ./target/debug/client 127.0.0.1:3001
   ```
   若提示 `vhci_hcd module not loaded`，可先执行 `sudo modprobe vhci_hcd`。当前 `linux` backend 只做 readiness probe，真实 usbip/vhci attach 和 URB 注入仍是下一步实现项。
   `WHY_USB_VHCI_DEVID` 可覆盖 dry-run attach request 的 `devid`，默认是 `0x00010001`：
   ```bash
   WHY_USB_VHCI_BACKEND=linux WHY_USB_VHCI_DEVID=0x00010002 ./target/debug/client 127.0.0.1:3001
   ```
   也可以只跑 WSL/Linux VHCI probe，不连接 daemon：
   ```bash
   bash scripts/wsl_vhci_probe.sh
   ```

### 2. 预期行为 (Expected Output)

*   **Daemon 端** 应该输出类似：
    ```text
    INFO starting why_usb user-mode daemon
    INFO driver backend initialized successfully
    INFO listening for client connections bind_addr=0.0.0.0:3000
    INFO accepted client connection peer_addr=127.0.0.1:<Port>
    INFO received network frame request_id=1 frame_type=AttachRequest payload_len=0
    INFO protocol session attached state=Attached
    INFO received network frame request_id=2 frame_type=Request payload_len=26
    INFO received network frame request_id=4 frame_type=DetachRequest payload_len=0
    ```
*   **Client 端** 应该输出：
    ```text
    INFO starting why_usb Linux client
    INFO attempting to connect to server state=Connecting server_addr=127.0.0.1:3000
    INFO connected to server state=Connected
    INFO sending attach request request_id=1
    INFO received lifecycle frame request_id=1 frame_type=AttachResponse status=0
    INFO received attach descriptors vendor_id=1234 product_id=5678 configurations=1
    INFO sending control request request_id=2 standard_request=GetDescriptor descriptor_type=Some(Device)
    INFO completed GET_DESCRIPTOR descriptor_type=Some(Device) response_len=18
    INFO sending control request request_id=3 standard_request=GetDescriptor descriptor_type=Some(Configuration)
    INFO completed GET_DESCRIPTOR descriptor_type=Some(Configuration) response_len=<ConfigLen>
    INFO sending control request request_id=4 standard_request=GetDescriptor descriptor_type=Some(Report)
    INFO completed GET_DESCRIPTOR descriptor_type=Some(Report) response_len=63
    INFO mock HID report descriptor validated report_descriptor_len=63
    INFO sending control request request_id=5 standard_request=SetAddress descriptor_type=None
    INFO completed control request standard_request=SetAddress
    INFO sending control request request_id=6 standard_request=SetConfiguration descriptor_type=None
    INFO completed control request standard_request=SetConfiguration
    INFO mock USB enumeration completed
    INFO mock VHCI session attached state=Attached
    INFO mock HID keyboard input report modifiers=0 keycodes=[4, 0, 0, 0, 0, 0]
    INFO mock HID keyboard input report modifiers=0 keycodes=[0, 0, 0, 0, 0, 0]
    INFO mock HID keyboard input report modifiers=0 keycodes=[40, 0, 0, 0, 0, 0]
    INFO mock HID keyboard input report modifiers=0 keycodes=[0, 0, 0, 0, 0, 0]
    INFO sending frame to server request_id=7 payload_len=26
    INFO received network frame request_id=7 frame_type=Response status=0 payload_len=32
    INFO mock VHCI processed URB urb_len=32
    ```
*   随后，Client 会模拟一个 Linux 本地的 USB 设备（例如插入了虚拟 U 盘），每 5 秒向 Daemon 发送一次 `Mock URB from Linux Client` 负载。
*   **交互输出**:
    *   Client 会先发送 `AttachRequest`，Daemon 返回带 USB descriptor set 的 `AttachResponse` 后才开始发送数据面 request。
    *   Client 会用 endpoint 0 control request 模拟枚举序列：`GET_DESCRIPTOR(Device)`、`GET_DESCRIPTOR(Configuration)`、`GET_DESCRIPTOR(Report)`、`SET_ADDRESS`、`SET_CONFIGURATION`。
    *   Daemon 在 `SET_CONFIGURATION(1)` 成功后会发送 interrupt IN `Event`，按 `WHY_USB_MOCK_HID_KEYS` 模拟 HID keyboard 的按下和释放报告。
    *   Client 发送的是 `protocol::Frame` 编码后的 request，而不是裸字符串。
    *   Daemon 接收到数据面 request 后，会先进入 endpoint transfer queue；同一 endpoint 内保持 FIFO，不同 endpoint 之间轮询 dispatch。
    *   Mock 协议层已定义 `CancelRequest` / `CancelResponse` / `ResetRequest` / `ResetResponse`，普通 response 的 `status` 可表达 `Failed`、`Cancelled`、`Timeout`、`Stall` 和 `Reset`。
    *   Daemon 可按 request id 取消 pending transfer，也可按 endpoint reset 清理 pending queue；被 reset 的 pending transfer 会以 `Reset` 状态完成。
    *   Daemon 可通过 `WHY_USB_MOCK_TRANSFER_OUTCOMES` 注入 fault，例如 `7=timeout,8=stall,9=short:4`；这让 timeout、stall 和 short packet 在本地 mock 联调中可重复验证。
    *   Client 可通过 `WHY_USB_MOCK_BULK_BYTES` 发送带 checksum 的 mock bulk payload；Daemon echo 后，Client 会校验 payload 完整性并注入 mock VHCI。
    *   Dispatch 后会通过 FFI (`rx_ring_push_frame`) 将完整 frame 写入 C++ 驱动的 `RX_Ring`。
    *   Mock driver pump 会把 RX frame 转移到 `TX_Ring`，daemon 再生成同 request id 的 mock response 发回 client。
    *   Client 解码 response 后，把 response payload 注入 mock `VhciAdapter` channel。
    *   Client 已有 `WHY_USB_VHCI_BACKEND=mock|linux` 选择；`linux` 会探测 `/sys/module/vhci_hcd`、`/sys/devices/platform/vhci_hcd*/status` 和 `attach`，解析 free port，并按内核 sysfs 格式建模 `port sockfd devid speed` attach request。连接 daemon 后会 dry-run 记录 socket fd handoff 计划；真实 sysfs 写入和 usbip 协议 socket handoff 尚未启用。
    *   Protocol 层已建模 Linux `vhci_hcd` socket 需要的 USB/IP `CMD_SUBMIT`、`RET_SUBMIT`、`CMD_UNLINK`、`RET_UNLINK` 包；Daemon 可通过 `WHY_USB_DAEMON_PROTOCOL=usbip` 进入最小 USB/IP socket loop，处理 mock HID keyboard 的 endpoint 0 枚举 control transfer、`CMD_UNLINK`，以及 endpoint `0x81` interrupt IN 报告。`scripts/local_usbip_mock.sh` 会启动该模式并用二进制 USB/IP submit/unlink 包验证 descriptor 与 HID report。
    *   有限帧联调模式下，Client 收到最后一个 response 后发送 `DetachRequest`，Daemon 返回 `DetachResponse` 并清理 session。
    *   Session 退出时，Daemon 会显式清理 endpoint transfer queue；本地压测脚本会在多轮 attach/detach 中检查该清理日志。

---

## 阶段二：生产环境极限测试 (Production / Phase 5 Acceptance)

当在 Windows 上安装了真实的 KMDF 驱动，并在 Linux 端配置了 `/dev/vhci` 后，必须进行以下真实场景测试。这直接映射了我们立项时设定的 KPI：**“极限榨干网络与总线带宽，实现极低延迟”**。

### 1. 连通性测试 (Basic Connectivity)

*   **测试方法**: 在 Windows (Server) 端插入一个普通的 USB 鼠标。在 Linux (Client) 端绑定该设备。
*   **预期行为**:
    1.  Client 端通过 `lsusb` 能看到该鼠标设备。
    2.  鼠标在 Linux 桌面（或传导回虚拟机界面）中可以顺畅移动。
    3.  感受延迟：不应有明显的“粘滞感”或跳帧，整体延迟应与物理直连无异 (< 10ms)。

### 2. 高吞吐量测试 (High Throughput / Storage)

*   **测试方法**:
    1.  在 Server 端挂载一块 USB 3.0 或以上的 SSD 移动硬盘 / U 盘。
    2.  Client 端挂载后，使用 `fio` 进行测试：
        ```bash
        sudo fio --name=seqread --rw=read --direct=1 --ioengine=libaio --bs=1M --numjobs=1 --size=1G --runtime=60 --group_reporting --filename=/dev/sdX
        ```
*   **预期行为**:
    1.  传输过程中，Daemon 和 Client 的 CPU 占用率应该很低（得益于 Zero-copy 和 Tokio 异步）。
    2.  **核心指标**: 读写速度应达到物理直连速度的 **80% 以上**（例如物理直连 400MB/s，挂载后应不低于 320MB/s）。如果卡在 30MB/s (USB 2.0 速度) 或极低吞吐量，说明 Ring Buffer 批量处理或 TCP 缓冲区存在瓶颈。

### 3. 高频并发测试 (High Frequency Isoc / Webcam)

*   **测试场景说明**: Web 摄像头使用的是 USB Isochronous (同步) 传输模式。这种模式极度依赖低延迟和高频 URB 提交，是 `usbipd-win` 崩溃/卡顿的重灾区。
*   **测试方法**:
    1.  挂载一个 1080p 60fps 的 USB 摄像头。
    2.  在 Client 端使用 `ffplay /dev/video0` 或 `guvcview` 打开摄像头流。
*   **预期行为**:
    1.  视频流应立即出现，无绿屏。
    2.  在剧烈晃动摄像头时，画面不应出现严重的撕裂 (Tearing) 或掉帧 (Frame drops)。
    3.  *(高级)*：如果在极高负载下出现网络抖动，系统应优雅地丢弃过期的 Isoc 帧，而不是无限堆积导致内存爆炸或延迟越来越高（长达几秒的延迟）。

### 4. 稳定性与内存泄漏审查 (Stability & BSOD Check)

*   **测试方法**:
    1.  在 Windows Server 端，打开 **Driver Verifier** (`verifier.exe`)，选中我们编写的 `why_usb_vhci.sys` 驱动，开启标准验证和池跟踪（Pool Tracking）。
    2.  写一个脚本，疯狂地执行绑定/解绑设备操作，或者反复插拔物理 USB 设备 100 次以上。
    3.  在 Linux Client 端写一个脚本疯狂建立/断开 TCP 连接。
    4.  使用 Windows 性能监视器 (Perfmon) 观察 Daemon 的内存占用，使用 Poolmon 观察驱动的非分页池 (Non-paged pool) 占用。
*   **预期行为**:
    1.  **绝不蓝屏 (No BSOD)**。WDF 驱动必须安全处理 URB 取消 (Cancellation) 和设备意外移除 (Surprise Removal) 事件。
    2.  内存曲线应该平稳，没有持续增长的趋势。
    3.  当 Client 断开连接时，Server Daemon 和驱动能够干净地清理 Ring Buffer，不残留僵尸状态。
