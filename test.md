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
2. **启动 Server (Daemon)**:
   在终端 1 中运行守护进程：
   ```bash
   ./target/debug/daemon
   ```
3. **启动 Client**:
   在终端 2 中运行客户端：
   ```bash
   ./target/debug/client
   ```

### 2. 预期行为 (Expected Output)

*   **Daemon 端** 应该输出：
    ```text
    [Daemon] Starting why_usb user-mode daemon...
    [Driver] Mock KMDF Driver Initialized.
    [Driver] Shared Memory Allocated at: <Memory Address>
    [Daemon] Driver initialized successfully.
    [Daemon] Listening for Client connections on port 3000...
    [Daemon] Accepted connection from: 127.0.0.1:<Port>
    ```
*   **Client 端** 应该输出：
    ```text
    [Client] Starting why_usb Linux client...
    [Client] Attempting to connect to Server at 127.0.0.1:3000...
    [Client] Connected to Server!
    ```
*   随后，Client 会模拟一个 Linux 本地的 USB 设备（例如插入了虚拟 U 盘），每 5 秒向 Daemon 发送一次 `Mock URB from Linux Client` 负载。
*   **交互输出**:
    *   Client 发送后会打印：`[Client] Sending 26 bytes to Server...`
    *   Daemon 接收到网络帧后，会通过 FFI (`rx_ring_push`) 将其写入 C++ 驱动的 `RX_Ring`。
    *   *(注：由于目前的 mock 没有真实设备的返回流，你可以通过修改 `daemon/src/main.rs` 注入反向测试流，但这超出了当前的基础连接测试范围)*。

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
