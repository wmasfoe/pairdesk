# 重构文档索引

> 这个目录回答：**Tauri → GPUI 重构做到哪一步了？验收标准是什么？**

## 文件

| 文件 | 回答什么问题 |
|---|---|
| [`spec.md`](spec.md) | 重构目标、验收标准、行为等价清单 |
| [`progress.md`](progress.md) | 当前进度、已完成/进行中/待办 |
| [`test-matrix.md`](test-matrix.md) | 旧行为 → 新实现的对照表，每个行为对应哪个单测 |

## 核心原则

1. **行为等价优先于代码美观**：迁移后的 App 行为必须与旧版逐项一致，包括文案、默认值、错误提示。
2. **先测后写**：每个行为先有单测（在 `pairdesk-shell`），再写 GPUI 实现。
3. **单测即规格**：`cargo test -p pairdesk-shell` 全绿 = 业务逻辑迁移完成。

## 快速验收

```bash
# 领域层全部行为（81 个单测）
cargo test -p pairdesk-shell

# 引擎层不受影响
cargo test --lib -p pairdesk-core

# GPUI 壳能编译并起窗口
cargo build -p pairdesk-ui
```
