# 架构文档索引

> 这个目录回答：**代码应该放在哪一层？依赖可以指向谁？**

## 文件

| 文件 | 回答什么问题 |
|---|---|
| [`clean-architecture.md`](clean-architecture.md) | 分层规则、依赖方向、每层的职责与禁止事项 |
| [`crate-map.md`](crate-map.md) | 每个 crate 的定位、依赖关系图、新增 crate 的判定流程 |
| [`gpui-constraints.md`](gpui-constraints.md) | GPUI 的技术约束与已踩过的坑（**改 UI 层前必读**） |

## 一句话摘要

- **依赖只能向内**：`ui` → `shell`（领域）→ `core`（引擎）
- **`pairdesk-shell` 是领域层**：纯 Rust，零 UI 框架依赖，**全部业务行为在这里，且必须可 `cargo test`**
- **`pairdesk-core` 是引擎层**：协议/采集/编码/加密/传输/注入，不知道"界面"存在
- **UI 层（Tauri 或 GPUI）是适配器**：只做「状态 → 像素」和「用户操作 → 命令」，**不含业务判断**
- 换 UI 框架 = 换最外层，领域层与引擎层零改动

## GPUI 三大崩溃级陷阱（改 UI 层前先看）

| 陷阱 | 症状 | 详见 |
|---|---|---|
| **在 `cx.spawn` 里用 tokio 生态的异步库** | 点一下 App 直接退出，无任何提示 | [§3](gpui-constraints.md#3-异步执行器不是-tokio--别在-cxspawn-里用-tokio-生态的异步库) |
| **在 `render()` 里 `cx.notify()` 拉帧** | 帧率从 20fps 塌到 0.2fps | [§4](gpui-constraints.md#4-帧泵不要在-render-里-cxnotify) |
| **忘记 `cx.drop_image()` 释放旧纹理** | GPU 内存无限增长直至崩溃 | [§5](gpui-constraints.md#5-纹理必须显式释放) |

## 判定口诀

> 写代码前问自己：**这段逻辑能在没有窗口的情况下被单测验证吗？**
> 能 → 放 `shell` 或 `core`。
> 不能（必须画出来才知道对不对）→ 才放 UI 层，且只做渲染与事件转发。
