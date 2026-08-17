# PairDesk

两人 P2P 远程协助 App（Rust + Tauri）。

- 场景：两台 Mac 同一 WiFi 局域网内互相远程协助
- 形态：一个 App 两种模式——被控端（等别人连）/ 控制端（去连别人）
- 技术栈：`pairdesk-core`（核心引擎，纯 Rust）→ `ui-kit`（前端组件库）→ `app`（Tauri 界面）
- 设计文档：[docs/design.md](docs/design.md)

## 开发里程碑
- [x] **M0 核心引擎**：采集/编码/加密/传输/注入全链路（Linux 虚拟屏幕端到端验证 9/9 通过）
- [ ] M1 Tauri 界面骨架
- [ ] M2 macOS 实机
- [ ] M3 体验打磨（Bonjour 发现/二维码/权限引导）
- [ ] M4 Windows 后端

## 快速体验（Linux 验证版）
```bash
# 终端 1: 被控端（需要 X 环境）
pairdesk serve --port 8888 --password 123456
# 终端 2: 控制端（收 10 帧画面，存到 /tmp/frames）
pairdesk connect 127.0.0.1:8888 --password 123456 --frames 10 --dump-dir /tmp/frames
```
