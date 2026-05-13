# **why\_usb: 核心开发与实施指南 (致 Jules)**

Jules，欢迎加入 **why\_usb** 项目。

本项目的核心目标是解决现有 usbipd-win 在高吞吐量设备（如高清摄像头、高速外部存储、逻辑分析仪等）上遭遇的性能瓶颈。我们的核心指标是：**极限榨干网络与总线带宽，实现极低延迟。** 为了实现这个目标，我们采用了 **C++ (Windows 内核层) \+ Rust (用户态网络层)** 的混合架构。

这份文档是你的实施路线图，请严格按照以下阶段和技术规范进行开发。

## **核心指导原则 (The "Why" in why\_usb)**

1. **数据面与控制面分离**：控制命令（如设备枚举、挂载/卸载）可以使用常规的 IOCTL 和 JSON 序列化；但**数据面（URB 传输）必须是零拷贝（Zero-copy）和无锁（Lock-free）的**。  
2. **拒绝频繁的上下文切换**：usbipd-win 慢的一个关键原因是每个 URB (USB Request Block) 都在内核与用户态之间频繁穿梭。我们必须使用 **共享内存环形缓冲区 (Shared Memory Ring Buffer)** 批量处理数据。  
3. **Rust 的并发与安全**：网络 I/O 必须 100% 异步（基于 Tokio）。使用 Rust 来防止内存泄漏和竞态条件。

## **第一阶段：工程脚手架与 FFI 桥接搭建**

你的第一步是让 Rust 和 C++ 能够顺畅地“交谈”。

### **1.1 目录结构初始化**

请按照以下结构建立仓库：

why\_usb/  
├── driver/           \# C++ Windows WDF 虚拟驱动代码  
│   ├── src/  
│   ├── inc/  
│   └── why\_usb\_vhci.sln  
├── daemon/           \# Rust 用户态守护进程 (Server 端)  
│   ├── src/  
│   ├── build.rs  
│   └── Cargo.toml  
├── client/           \# Rust 客户端 (Linux/WSL2 端)  
│   ├── src/  
│   └── Cargo.toml  
├── protocol/         \# Rust/C++ 共享的网络协议与结构体定义  
└── agent.md          \# 本文档

### **1.2 配置 CXX 桥接**

* **任务**：使用 Rust 的 cxx crate 构建安全边界。不要手写危险的 C-ABI extern "C"，除非必要。  
* **要求**：在 daemon/build.rs 中配置 CMake 构建流程，使 cargo build 能够自动编译 C++ 驱动控制库。  
* **里程碑**：Rust 代码能够成功调用一个简单的 C++ 函数 init\_vhci\_driver() 并返回状态。

## **第二阶段：Windows 内核驱动开发 (C++ / WDF)**

这是最硬核的部分。我们需要模拟一个 USB 主机控制器 (VHCI) 或直接模拟总线。

### **2.1 WDF 驱动框架搭建**

* **任务**：编写 KMDF (Kernel-Mode Driver Framework) 驱动，使其能在 Windows 设备管理器中注册为虚拟的 USB 根集线器。  
* **拦截 URB**：实现对 Windows USB 栈发出的 URB（USB Request Block）的拦截和解析。

### **2.2 核心优化：共享内存环形缓冲区 (Ring Buffer)**

**这是战胜 usbipd-win 的关键！**

* **任务**：在驱动初始化时，申请一块非分页池内存（Non-paged pool），并将其映射到 daemon 的用户态地址空间。  
* **结构设计**：  
  * 创建两个 SPSC (Single-Producer, Single-Consumer) 环形缓冲区：TX\_Ring 和 RX\_Ring。  
  * TX\_Ring：驱动将截获的 URB payload 写入，通知用户态读取。  
  * RX\_Ring：用户态将网络接收到的远端设备响应写入，通知驱动完成 URB。  
* **同步机制**：使用 Event (事件对象) 通知，而不是轮询（Spinlock），以降低 CPU 占用。但支持在极高负载下切换为短暂轮询。

## **第三阶段：用户态守护进程开发 (Rust / Tokio)**

这一层负责将环形缓冲区里的数据以最快速度搬运到网络上。

### **3.1 内存映射与解析 (Rust 侧)**

* **任务**：使用 winapi 或 windows-rs crate 获取 C++ 驱动分配的共享内存句柄，并将其安全地封装为 Rust 的 &\[u8\] 切片或自定义结构体。  
* **序列化策略**：**禁止在数据面使用 JSON/Protobuf。** 直接将 URB 数据结构通过网络字节序（或使用极其轻量级的 bincode / 裸指针强转 bytemuck）打包发送。

### **3.2 极致的网络传输层**

* **任务**：基于 Tokio 建立异步 TCP Server。  
* **优化点**：  
  * 开启 TCP\_NODELAY (禁用 Nagle 算法) 降低小包延迟。  
  * 调整 socket 缓冲区大小 (SO\_RCVBUF, SO\_SNDBUF) 到最大。  
  * 实现基于 Length-Prefixed 的快速帧解码器 (tokio-util 的 LengthDelimitedCodec)。  
  * **高级目标**：如果 TCP 仍有瓶颈，预留切换到 QUIC (基于 quinn crate) 或原生 UDP \+ 可靠重传机制的接口。

## **第四阶段：Linux / WSL2 客户端开发 (Rust)**

这部分相对简单，因为 Linux 内核已经有了现成的 vhci-hcd (USB/IP 虚拟主控制器驱动)。

### **4.1 Linux USB/IP 协议适配**

* **任务**：分析 Linux vhci-hcd 期望的通信格式。客户端需要将来自 why\_usb Server 的高速私有协议，翻译或解包后喂给 Linux 内核的 vhci-hcd。  
* **连接内核**：通过 netlink 或传统的 /dev/vhci (取决于 Linux 版本) 将自己注册为 USB/IP 设备。

### **4.2 客户端网络栈**

* **任务**：与 Server 端对称，使用 Tokio 处理高并发的网络接收，并将数据零拷贝地写入 Linux 内核。

## **阶段验收与测试计划 (Jules 的 Checklists)**

在提交 PR 之前，请确保通过以下测试：

* \[ \] **连通性测试**：插上一个普通的 USB 鼠标，在 Client 端是否能顺畅移动？延迟是否可以接受？  
* \[ \] **高吞吐量测试**：挂载一个 USB 3.0 U盘。使用 fio 或 CrystalDiskMark 测试读写速度。**目标：达到物理直连速度的 80% 以上。**  
* \[ \] **高频并发测试**：挂载 USB 摄像头 (1080p 60fps)。观察视频流是否出现卡顿、撕裂或掉帧？(usbipd-win 通常在这里阵亡)。  
* \[ \] **内存泄漏检查**：拔插设备 100 次，监控 Driver 非分页池内存和 Rust 进程内存是否有增长。  
* \[ \] **蓝屏 (BSOD) 审查**：使用 Driver Verifier (驱动程序验证程序) 运行 Windows 驱动，确保没有非法的内存访问或死锁。

Jules，这是一场针对性能的战役。不要在热路径 (Hot Path) 上留下任何多余的内存分配 (Box::new, malloc, clone)。我们期待看到 why\_usb 的跑分结果。祝编码顺利！