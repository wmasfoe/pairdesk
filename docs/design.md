# PairDesk 设计方案（定稿：Tauri）

> 两人 P2P 远程协助 App。定位：**简单、无脑、两个人互相帮忙**。
> 参考 RustDesk 架构思路，砍掉服务器/ID 系统/NAT 穿透等一切重功能，只保留闭环核心。
> 技术栈：**Rust 核心引擎 + Tauri 2（Web 前端界面）**，采集/注入走平台原生接口。

---

## 1. 定位与场景

- **场景**：两台 Mac 在同一 WiFi（局域网），互相远程协助（帮装软件 / 帮改设置 / 帮看问题）
- **形态**：一个 App，两种模式——`被控端`（等别人连我）／`控制端`（我去连别人）
- **V1 明确不做**：公网穿透、中继服务器、ID 注册、音频通话、文件传输、白板、多用户
- **演进**：V1 局域网纯直连（零服务器）→ V2 可选极简信令（解决跨网络，跑在用户现有 VPS 上）

## 2. 技术栈

| 项 | 选型 | 说明 |
|---|---|---|
| 核心引擎 | Rust（workspace crate `pairdesk-core`） | 零 UI 依赖，纯逻辑 |
| UI | **Tauri 2** + React 19 + TypeScript + Vite | 一个安装包三平台；前端组件库独立成 `packages/ui-kit` |
| 屏幕采集 | macOS: ScreenCaptureKit ｜ Windows: DXGI ｜ Linux: X11/XShm | 平台原生 |
| 输入注入 | macOS: CGEvent ｜ Windows: SendInput ｜ Linux: XTest | 平台原生 |
| 编码 | JPEG（纯 Rust 软编） | V1 全量帧；V1.5 差分块降低流量 |
| 加密 | XChaCha20-Poly1305（密钥经密码派生） | 轻量、跨端一致 |
| 发现（V1.5） | Bonjour/mDNS 局域网自动发现 | 同网设备自动列出 |
| 打包 | `tauri build`（.dmg / .exe+MSI / .AppImage） | 官方工具链 |

## 3. 三层架构总览

```
┌──────────────────────────────────────────────────────┐
│  用户 UI 层  app/ 前端(src+src-tauri)  业务页面/交互     │
│            React 页面 ↔ Tauri commands/events 桥接     │
│  ── 依赖 ──────────────────────────────────────────  │
│  基础 UI 层  packages/ui-kit            Web 通用组件库  │
│            (React+TS,与业务无关:按钮/输入/卡片/二维码..)   │
│  ── 依赖 ──────────────────────────────────────────  │
│  核心引擎层  crates/pairdesk-core      协议/采集/编码/   │
│                             注入/会话(零 UI 依赖,纯 Rust) │
└──────────────────────────────────────────────────────┘
   ▲ 事件推送到前端(Tauri event)   ▼ 前端发命令(Tauri command)
        pairdesk-discovery（Bonjour 发现,可选,可并入 core）
```

**层间铁律：**
- **核心层不知道 UI 存在**：只暴露 `CoreHandle`（命令）+ `CoreEvent` 事件流；Tauri 侧做一层薄桥接（command/event 直达 channel）
- **UI 层不碰平台细节**：不直接调系统 API，一切经核心层
- **依赖单向**：app → ui-kit → core

## 4. 核心引擎层（crates/pairdesk-core）

### 4.1 模块划分

| 模块 | 职责 | 关键抽象 |
|---|---|---|
| `protocol` | 帧协议、握手、认证、加密 | `Frame{type,len,payload}`、`Handshake`、`Cipher` |
| `transport` | TCP 连接管理、心跳、重连 | `Connection`（读写分离） |
| `capture` | 屏幕采集（平台差异在此收敛） | `trait ScreenCapturer` |
| `encode` | 画面编码、分辨率/质量自适应 | `Encoder`（JPEG） |
| `input` | 输入注入（平台差异在此收敛） | `trait InputInjector` |
| `session` | 会话状态机（角色/握手顺序/事件派发） | `Session`、`Role::{Host,Viewer}` |
| `events` | 事件通道（core→上层的唯一出口） | `CoreEvent`、`ControlCommand` |

### 4.2 核心 trait（平台插头）

```rust
pub trait ScreenCapturer: Send {
    fn capture(&mut self) -> anyhow::Result<CapturedFrame>;
    fn display_size(&self) -> (u32, u32);
}

pub trait InputInjector: Send {
    fn move_mouse(&mut self, x: f64, y: f64) -> anyhow::Result<()>;
    fn press_button(&mut self, btn: MouseButton) -> anyhow::Result<()>;
    fn scroll(&mut self, dx: f64, dy: f64) -> anyhow::Result<()>;
    fn key(&mut self, key: Key, down: bool, mods: Modifiers) -> anyhow::Result<()>;
}
```

实现按 `#[cfg(target_os)]` 门控分文件：
- `capture/macos.rs`（ScreenCaptureKit）、`capture/windows.rs`（DXGI）、`capture/linux.rs`（X11）
- `input/macos.rs`（CGEvent）、`input/windows.rs`（SendInput）、`input/linux.rs`（XTest）

### 4.3 会话流程（协议时序）

```
[TCP connect :8888]
Viewer ──HELLO(协议版本)──────────► Host
Viewer ◀─HELLO_ACK(版本OK)──────── Host
Viewer ──AUTH(密码,带盐校验)──────► Host   ← 密码不落盘
Viewer ◀─AUTH_OK(密钥种子)──────── Host   ← 双向密钥由密码+随机数派生
        ── 之后全部帧加密 ──
Viewer ◀─SIZE(屏幕尺寸)─────────── Host
Viewer ──INPUT(鼠标/键盘)─────────► Host   ⟲ 循环
Viewer ◀─FRAME(JPEG 帧)────────── Host   ⟲ 循环
```
心跳 15s / 3 次失联即断，断开可一键重连。

### 4.4 线程模型

```
        ┌────────────── core 内部 ──────────────┐
        │  capture线程: 采屏→编码→发送            │
        │  receive线程: 收帧→解析→注入/事件        │
        │  heartbeat线程: 周期心跳                 │
        └──────────────────────────────────────┘
                  │ CoreEvent (crossbeam channel)
                  ▼
         Tauri 桥接层 → 前端 (window events)
```

### 4.5 对外接口

```rust
pub struct CoreHandle { /* Sender<ControlCommand> */ }
impl CoreHandle {
    pub fn start_host(port: u16, password: String) -> Result<CoreHandle>;
    pub fn connect(addr: SocketAddr, password: String) -> Result<CoreHandle>;
    pub fn stop(self);
    pub fn set_quality(&self, q: Quality);
    pub fn events(&self) -> Receiver<CoreEvent>;
}

pub enum CoreEvent {
    ScreenFrame(Vec<u8> /*jpeg*/),
    Size { w: u32, h: u32 },
    PeerConnected, PeerDisconnected,
    AuthResult { ok: bool, reason: Option<String> },
    Stats { fps: u32, kbps: u32, ping_ms: u32 },
    Error(String),
}
```

## 5. 基础 UI 层（packages/ui-kit）

> React + TS 通用 Web 组件库，与业务无关。保障：业务页面代码里不出现裸 HTML 控件。

- **主题**：亮/暗两套（跟随系统），统一 token（色板/字号/圆角/间距）
- **组件**：`Button / TextField / StatusDot / Toast / Switch / Card / DeviceRow / Dialog / QRCode / VideoView`
- 组件纯受控（props 注入状态），单测 + 截图测试

## 6. 用户 UI 层（app/）

```
App 根：模式选择（两个大卡片）
 ├─ 被控端页   大二维码(pairdesk://ip:port#pw) + 密码大字
 │            本机IP显示 + 连接状态卡 + 断开按钮
 │            权限引导卡(macOS TCC 缺失时置顶)
 └─ 控制端页   设备列表(Bonjour 自动发现 + 手动 IP)
              输入密码 → 画面区(等比缩放/全屏)
              工具栏(画质/帧率/延迟/码率/断开)
```

**交互细节：**
- 鼠标坐标等比映射到远端屏幕；滚轮/按键/修饰键原样转发
- 画面：JPEG 解码（前端 canvas/image）→ 上屏，收帧即更新，窗口缩放保比例
- 密码：被控端每次启动随机 6 位（可固定）；会话结束失效
- 权限引导：检测 macOS TCC，缺失时指引两步开启（系统设置→隐私与安全性→屏幕录制/辅助功能）

## 7. 平台支持矩阵

| 能力 | macOS（优先） | Windows | Linux |
|---|---|---|---|
| 采集 | ScreenCaptureKit | DXGI | X11 XShm |
| 注入 | CGEvent | SendInput | XTest |
| 发现(V1.5) | Bonjour | mDNS | Avahi |
| 打包 | .app/DMG | exe/MSI | AppImage |
| 前端运行时 | 系统 WKWebView | WebView2 | WebKitGTK(已装) |

## 8. 工作区目录结构

```
pairdesk/
├── Cargo.toml            # workspace (crates/*)
├── crates/
│   ├── pairdesk-core/    # 核心引擎层(纯 Rust)
│   └── pairdesk-app/     # Tauri 应用(壳+桥接,依赖 core)
├── packages/
│   └── ui-kit/           # 基础 UI 层(React+TS 组件库)
├── app/                  # 用户 UI 层(前端页面,依赖 ui-kit)
├── docs/                 # 设计/里程碑/权限指南/协议说明
├── scripts/              # 一键编译与打包脚本
└── tests/e2e/            # 端到端集成测试(双实例互连)
```

> 说明：Tauri 项目惯例是把 Rust 壳(src-tauri)与前端同目录。此处为保持三层清晰，
> 将前端页面放 `app/src`、Tauri Rust 壳放 `crates/pairdesk-app`（tauri.conf.json 指向 ./app/dist）。

## 9. 里程碑与验证方式（分层诚实）

| 里程碑 | 内容 | 验证方式 |
|---|---|---|
| **M0 核心链路** | workspace + core 在 Linux 跑通（采集/编码/协议/加密/注入全模块） | 无头 VM：Xvfb 虚拟屏幕 + CLI 双进程互连，断言画面帧到达+输入注入生效；协议/加密/状态机单测 |
| **M1 UI 骨架** | Tauri 应用出窗口、ui-kit 基础组件、双页面雏形、core↔前端桥接 | VM 上 `tauri dev` + Xvfb 截图断言页面渲染；组件单测 |
| **M2 macOS 实机** | macos 采集/注入后端 + `tauri build` 出 .app/.dmg | 用户在 Mac 上一键编译 + 双机实测（我无法模拟 macOS 实机，此项以用户验证为准） |
| **M3 体验打磨** | Bonjour 发现、二维码、权限引导、全屏 | 用户 Mac 双机实测 + 截图对比 |
| **M4 Windows** | windows 后端 + 打包 | 交叉编译 + Windows 实机（无环境则出 beta 给朋友实测） |

## 10. 风险清单

| 风险 | 等级 | 对策 |
|---|---|---|
| macOS TCC 权限导致首次使用卡壳 | 中 | 权限引导页面做扎实，文档写清两步开启 |
| 纯软编 JPEG 大分辨率 CPU 高 | 低 | V1 目标 1080p/30fps，画质/帧率可调；V1.5 差分块 |
| Windows 实机验证环境缺失 | 低 | 开发以 macOS 为目标，Windows 延后至有实测环境 |
| Tauri Linux 构建依赖 | 低 | webkit2gtk-4.1 已装；CI 用 tauri 官方 action |
| 前端画面解码性能 | 低 | JPEG 解码用浏览器原生能力，1080p 无压力；必要时转 MJPEG 流 |

## 11. 命名

**PairDesk**（pair=两人 + desk=桌面）。crate 前缀 `pairdesk-`，仓库名 `pairdesk`。