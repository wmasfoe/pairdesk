<p align="center">
  <a href="#pairdesk">
    <img width="280" alt="PairDesk" src="docs/design/logo/cat-signal-1024.png">
  </a>
</p>

# PairDesk

<p align="center">
  两人 P2P 远程协助 App —— 一台设备当<b>被控端</b>，另一台当<b>控制端</b>，同 WiFi 直连、异地跨网互帮。
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#网络架构">架构</a> ·
  <a href="#安装">Install</a> ·
  <a href="#快速体验">Quick Start</a> ·
  <a href="#开发里程碑">Milestones</a> ·
  <a href="#license">License</a>
</p>

<p align="center">
  <a href="https://github.com/wmasfoe/pairdesk/releases">
    <img src="https://img.shields.io/badge/platform-macOS_%7C_Windows_%7C_Linux-blue?style=flat-square" alt="Platforms">
  </a>
  <a href="https://github.com/wmasfoe/pairdesk/releases">
    <img src="https://img.shields.io/badge/built_with-Rust_%2B_Tauri-orange?style=flat-square&logo=rust&logoColor=white" alt="Built with Rust + Tauri">
  </a>
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License">
</p>

---

## Features

### 两种模式，一端切换

- **被控端**：生成会话码 + 密码，打开「允许远程控制」，等对方连入
- **控制端**：输入对方的会话码 + 密码，自动选择最优链路连过去
- 角色**不固定**——每台设备随时可当被控端，也可当控制端

### 三路传输，自动择优

| 场景 | 路径 | 特点 |
|------|------|------|
| 同一 WiFi / 局域网 | TCP 直连 | 低延迟，不经服务器 |
| 异地跨网 | QUIC 打洞 | 优先尝试 NAT 打洞直连 |
| 打洞失败 / 网络复杂 | 中继服务器 | 双方主动连 relay，逐字节透明转发兜底 |

控制端无感择优：QUIC 打洞优先，失败自动降级中继，不中断会话。

### 安全

- **端到端加密**：ChaCha20-Poly1305，中继服务器只见密文、无法解密内容
- **会话码 + 密码**双重校验，密码不落盘
- 被控端有「允许远程控制」总开关，随时可断

### 形态

- `pairdesk-core`：核心引擎（纯 Rust：采集 / 编码 / 加密 / 传输 / 注入）
- `pairdesk-relay`：中继服务器（独立部署 VPS，**不打包进客户端**）
- `ui-kit` / `app`：前端界面（React + Tauri）
- CLI：`pairdesk` / `pairdesk-relay` 三平台可用，另附 **MUSL 全静态版**（任何 Linux 免 glibc 直接跑）

## 网络架构

```
同一WiFi:  被控端 ──TCP直连── 控制端        （低延迟，不走服务器）
异地跨网:  被控端 ──┐              ┌── 控制端   （双方主动连中继，逐字节透明转发）
                    └── relay(VPS) ─┘
```

## 安装

从 [Releases](https://github.com/wmasfoe/pairdesk/releases) 下载对应平台的安装包：

| 平台 | 产物 |
|------|------|
| macOS | `PairDesk_*.dmg`（内含 .app，需在「隐私与安全」授予屏幕录制 / 辅助功能权限） |
| Windows | `PairDesk_*.msi` |
| Linux | `.deb` / `.AppImage` / `.rpm` |
| CLI（三平台） | `pairdesk-<OS>-<arch>` + `pairdesk-relay-<OS>-<arch>` |
| CLI（Linux 静态） | `pairdesk-linux-x86_64-musl`（零 glibc 依赖） |

### 一键安装（未签名，自动处理隔离/权限）

**macOS**（自动下载最新 dmg → 装到 /Applications → 解除 quarantine，不再报「已损坏」）：

```bash
curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-macos.sh | bash
```

**Linux**（默认 .deb 需 sudo；`--appimage` 免 root 装到 ~/.local/bin）：

```bash
# Debian/Ubuntu（.deb + 自动补依赖）
curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-linux.sh | sudo bash
# 免 root（AppImage）
curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-linux.sh | bash -s -- --appimage
```

**Windows**（PowerShell，msiexec 静默安装，**需管理员 PowerShell**）：

```powershell
irm https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-windows.ps1 | iex
```

## 快速体验

```bash
# 0) 起中继服务器（模拟 VPS；同一 WiFi 直连可跳过）
./target/debug/pairdesk-relay 8989

# 1) 被控端（同一 WiFi 直连；需要图形环境）
pairdesk serve --port 8888 --password 123456
#    异地跨网时：
pairdesk serve --relay 1.2.3.4:8989 --sid myroom --password 123456

# 2) 控制端（收 10 帧存盘）
pairdesk connect 127.0.0.1:8888 --password 123456 --frames 10 --dump-dir /tmp/frames
#    异地跨网时：
pairdesk connect 127.0.0.1:8888 --relay 1.2.3.4:8989 --sid myroom --password 123456 --frames 10
```

## 开发里程碑

- [x] **M0 核心引擎**：直连 9/9 验收（单测 + Xvfb 端到端）
- [x] **M0.5 中继链路**：relay 服务器 + 客户端中继模式，异地透明桥接 7/7 验收
- [x] **QUIC 传输基座**：tokio + quinn，跨进程可靠传输验证
- [x] **打洞落地**：relay 信令交换 + QUIC 承载完整会话，四链路全绿
- [x] **自动择一**：QUIC 打洞优先、失败降级中继（auto-e2e 8/8）
- [x] **M1 App 骨架**：Tauri 壳 + 前端（会话码 / 密码 / 允许开关 / 自动路径）
- [ ] 三路全自动（同网 TCP 直连并入自动择一，含竞速优先）
- [ ] **M2 macOS 真机**：core-graphics 采集 + CGEvent 注入已实现、CI 编译绿，待实机权限验证
- [ ] M3 体验打磨（权限引导、二维码）
- [ ] M4 Windows 后端（DXGI 采集 + SendInput 注入）

## License

MIT

---

<p align="center">
  Copyright © 2026 PairDesk. All rights reserved.
</p>
