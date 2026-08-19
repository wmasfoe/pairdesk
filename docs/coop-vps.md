# PairDesk — 多 VPS/Agent 协作指南

> 本文用于让其他 VPS 上的 Hermes/AI agent（或协作者）明白**怎样配合 PairDesk 的开发与验证**。
> 项目：**PairDesk** —— 双人远程协助（Rust 内核 + Tauri 壳）。仓库 `wmasfoe/pairdesk`（私有，需访问）。

## 一、项目结构（一眼掌握）

```
crates/pairdesk-core    内核：同网TCP直连 / QUIC打洞直连 / relay中继 / 自动择一
crates/pairdesk-relay   中继服务器（透明字节桥接，独立部署在 VPS，不带进客户端）
crates/pairdesk-app     Tauri 2 桌面壳 + IPC 桥 + 前端(React/Vite)
app/                    前端业务页（会话码+密码+允许开关+自动择一）
tests/                  各链路 e2e 脚本
```

## 二、让其他 VPS 参与的方式（按你能扛哪块挑）

### 1. 干净环境跑编译/回归（最通用、最安全）
```bash
cargo build --workspace        # Linux ARM/x86 均可
cargo test                     # 预期单测 10/10
bash tests/e2e.sh              # 同网 TCP 直连 → 9/9
bash tests/relay-e2e.sh        # 中继链路 → 7/7
bash tests/quic-e2e.sh         # QUIC 打洞直连 → 4/4
bash tests/auto-e2e.sh         # 自动择一(QUIC→中继兜底) → 8/8
```
依赖：`libx11-dev libxcb1-dev libxtst-dev libxfixes-dev libxrandr-dev libxinerama-dev` + `Xvfb` + `ffmpeg`。
**结果如有任何红，先贴日志回来，别擅自改核心逻辑。**

### 2. 起一个公网 relay 中继（供双机异地打洞测试）
```bash
cargo run -p pairdesk-relay 8977   # 任意端口；字节级透明桥接，不解析协议
```
把 `公网IP:端口` 给我 —— 我在 App 的"中继地址"里填它做真机验证。

### 3. 交叉/双机验证（两台机器各跑一端）
一台当"被控端"、一台当"控制端"，都连同一个 relay + 相同会话码/密码，确认打洞链路通。

## 三、协定（务必遵守，否则会被打回）
- ❌ **绝不合并任何 PR** —— 所有 PR 必须用户亲自 review 后才能合
- ✅ Push 前必须**本地全绿**（连接收纪律；不靠 CI 当第一道校验）
- ✅ 不擅自动 `pairdesk-core` 的**核心协议/加密/会话**逻辑；要改先在分支自测
- ✅ 代码注释用中文；遵循现有 AGENTS.md（如有）

## 四、当前状态（属实，可复测）
- 内核四链路全绿：直连 9/9 · 中继 7/7 · QUIC 4/4 · 自动择一 8/8
- Tauri 壳 + IPC + 前端会话模型已就绪（编译/构建/冒烟通过）
- App 全链路 e2e 5/5（自测模式，双 App 实例真连收帧，画面正确）
- 待办：**macOS 实机**（ScreenCaptureKit 采集 + CGEvent 注入，需在 macOS 编译验证）
