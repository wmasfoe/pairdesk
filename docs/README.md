# PairDesk 文档索引

> **给 Agent 的读取顺序**：先读本文件 → 按任务类型进入子目录的 `README.md` → 再打开具体文件。
> 不要跳过索引直接 `find docs`，那会浪费上下文并读到过期文档。

## 目录结构

```
docs/
├── README.md            ← 你在这里（总索引）
├── architecture/        架构决策与分层规则
├── guides/              开发/构建/部署操作手册
├── refactor/            本次 Tauri → GPUI 重构的规格与进度
├── design/              视觉设计与交互稿（含图片资源）
└── legacy/              历史文档（已过期，仅作考古参考）
```

## 按任务查找

| 我要做什么 | 去哪里 |
|---|---|
| 理解整体分层、依赖方向、整洁架构规则 | [`architecture/README.md`](architecture/README.md) |
| 看 Tauri → GPUI 重构的验收规格与进度 | [`refactor/README.md`](refactor/README.md) |
| 跑起来 / 构建 / 打包 / 发版 | [`guides/README.md`](guides/README.md) |
| 部署中继服务器 | [`guides/relay-deploy.md`](guides/relay-deploy.md) |
| 改 UI 配色、字号、间距 | [`design/README.md`](design/README.md) |
| 查旧版（Tauri）的实现细节 | [`legacy/README.md`](legacy/README.md) |

## 核心事实速查

| 事实 | 值 |
|---|---|
| 当前 UI 框架 | Tauri 2 + React 19（迁移中 → GPUI） |
| 目标 UI 框架 | GPUI 0.2.2（锁版本，见 `architecture/`） |
| 支持平台 | macOS / Windows / Linux |
| 领域层（可单测） | `crates/pairdesk-shell` |
| 核心引擎（零 UI 依赖） | `crates/pairdesk-core` |
| 中继服务器 | `crates/pairdesk-relay` |
| 默认中继地址 | `35.209.148.235:8977` |
| 默认打洞端口 | `23517` |

## 文档编写约定

1. **每个目录必须有 `README.md` 作为索引**，列出该目录下所有文件及其用途。
2. 新增文档时，**必须**同步更新所在目录的 `README.md` 与本文档的「按任务查找」表。
3. 过期的文档移入 `legacy/`，不要留在原位误导。
4. 文件名用小写连字符：`session-state-machine.md`，不用 `SessionStateMachine.md`。
5. 每份文档开头用一行说明「这份文档回答什么问题」，方便快速判断是否要读下去。
