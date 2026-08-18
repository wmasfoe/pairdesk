# PairDesk

两人 P2P 远程协助 App（Rust + Tauri）。

- 场景：两台电脑互相远程协助——**同一 WiFi 局域网直连**，也支持**异地跨网（经中继服务器）**
- 形态：一个 App 两种模式——被控端（等别人连）/ 控制端（去连别人）
- 组成：
  - `pairdesk-core`：核心引擎（纯 Rust，采集/编码/加密/传输/注入）
  - `pairdesk-relay`：中继服务器（独立部署 VPS，信令牵线 + 透明桥接，**不进客户端安装包**）
  - `ui-kit` / `app`：前端界面（React + Tauri）
- 设计文档：[docs/design.md](docs/design.md)

## 网络架构
```
同一WiFi:  被控端 ──TCP直连── 控制端        （低延迟，不走服务器）
异地跨网:  被控端 ──┐              ┌── 控制端   （双方主动连中继，逐字节透明转发）
                    └── relay(VPS) ─┘
```

## 开发里程碑
- [x] **M0 核心引擎**：直连 9/9 验收（单测 + Xvfb 端到端）
- [x] **M0.5 中继链路**：relay 服务器 + 客户端中继模式，异地经中继透明桥接 7/7 验收
- [ ] M1 Tauri 界面骨架（前端已就绪，Tauri 壳待接）
- [ ] M2 macOS 实机
- [ ] M3 体验打磨（打洞优先+中继兜底、二维码、权限引导）
- [ ] M4 Windows 后端

## 快速体验（Linux 验证版）
```bash
# 0) 起中继服务器（模拟 VPS；同一 WiFi 直连可跳过）
./target/debug/pairdesk-relay 8989

# 1) 被控端（同一 WiFi 直连；需要 X 环境）
pairdesk serve --port 8888 --password 123456
#    异地跨网时：
pairdesk serve --relay 1.2.3.4:8989 --sid myroom --password 123456

# 2) 控制端（收 10 帧存盘）
pairdesk connect 127.0.0.1:8888 --password 123456 --frames 10 --dump-dir /tmp/frames
#    异地跨网时：
pairdesk connect 127.0.0.1:8888 --relay 1.2.3.4:8989 --sid myroom --password 123456 --frames 10
```
