# Crate 地图

> 这份文档回答：**有哪些 crate，谁依赖谁，新 crate 该不该建。**

## 依赖关系图

```
                              ┌──────────────────┐
                              │ pairdesk-ui      │  ← 适配器层（GPUI 壳）
                              │ 渲染 + 事件转发    │
                              └────────┬─────────┘
                                       ▼
                              ┌──────────────────┐
                              │ pairdesk-shell   │  ← 领域层
                              │ 会话状态机/输入映射 │
                              └────────┬─────────┘
                                       ▼
                              ┌──────────────────┐
                              │ pairdesk-core    │  ← 引擎层
                              │ 协议/采集/编码/传输 │
                              └──────────────────┘

                    ┌──────────────────┐
                    │ pairdesk-relay   │  ← 独立二进制，不参与上述依赖
                    └──────────────────┘
```

## Crate 清单

| Crate | 层 | 职责 | 依赖 | 单测 |
|---|---|---|---|---|
| `pairdesk-core` | 引擎 | 协议、采集、编码、加密、传输、打洞、注入 | 仅系统库 + 网络库 | ✅ 有 |
| `pairdesk-relay` | 独立服务 | 中继服务器，部署在 VPS | `pairdesk-core`（协议层） | ✅ 有 |
| `pairdesk-shell` | **领域** | 会话状态机、凭据、输入映射、更新、权限 | `pairdesk-core`（仅类型） | ✅ **必须全绿**（81） |
| `pairdesk-ui` | 适配器 | GPUI 壳：渲染 + 事件转发 | `pairdesk-shell`、`pairdesk-core`、gpui | ⚠️ 需窗口（18） |

## 二进制产物

| 二进制 | 来源 crate | 说明 |
|---|---|---|
| `pairdesk` | `pairdesk-core` (`src/bin/`) | CLI：`serve` / `connect` 子命令 |
| `pairdesk-relay` | `pairdesk-relay` | 中继服务器 |
| `pairdesk-ui` | `pairdesk-ui` | GPUI 桌面 App |

> Tauri 时代的 `app/`、`packages/ui-kit/`、`crates/pairdesk-app/` 及
> `package.json` / `pnpm-*` / `tsconfig.base.json` 已全部移除，仓库现在只有 Rust。

## 新增 crate 的判定

| 你想加的东西 | 该放哪 |
|---|---|
| 新的平台采集/注入实现 | `pairdesk-core/src/capture/` 或 `input/` 下的新平台文件 |
| 新的网络传输方式 | `pairdesk-core/src/` 新模块 |
| 新的业务规则（如"自动重连"、"连接历史"） | `pairdesk-shell/src/` 新模块 + 单测 |
| 新的窗口/组件/动画 | `pairdesk-ui/src/` |
| 新的独立服务（如发现服务器） | **新建 crate**，不得塞进 core |

## 分层验证命令

```bash
# shell 必须零 UI 框架依赖（输出为空才正确）
grep -rn "gpui\|tauri" crates/pairdesk-shell/src/ | grep -v "^.*//!"

# shell 不得做 I/O（输出为空才正确）
grep -rn "reqwest\|std::fs::\|TcpStream" crates/pairdesk-shell/src/

# core 必须零 UI 依赖（输出为空才正确）
grep -rn "gpui\|tauri" crates/pairdesk-core/src/
```

## 版本号约定

所有 crate 版本号统一（当前 `0.1.21`），发版时由 CI 从 tag 同步。新增 crate 必须跟随同一版本号，避免 `Cargo.lock` 出现多版本分裂。
