# 历史文档索引

> 这个目录回答：**旧版（Tauri）是怎么实现的。**

⚠️ **本目录文档已过期**，仅作考古参考。当前架构见
[`../architecture/`](../architecture/)，重构规格见 [`../refactor/`](../refactor/)。

## 文件

| 文件 | 内容 | 过期原因 |
|---|---|---|
| `design-tauri-v1.md` | Tauri 时代的完整设计方案 | 技术栈已改为 GPUI；UI 层结构（React/ui-kit）已不存在 |
| `coop-vps.md` | VPS 协作部署记录 | 部署流程未变，但引用路径可能失效 |

## 仍然有效的部分

`design-tauri-v1.md` 里以下内容**依然准确**（因为它们属于 `pairdesk-core`，未改动）：

- 协议设计（帧格式、握手、加密）
- 采集/注入的平台选型
- 传输策略（TCP 直连 / QUIC 打洞 / 中继兜底）
- 编码方案（JPEG）

失效的部分：

- UI 技术栈（Tauri 2 + React 19 + Tailwind → GPUI）
- 分层结构（三层 → 四层，新增领域层 `pairdesk-shell`）
- 前端包结构（`app/` + `packages/ui-kit/` 已废弃）

## 迁移映射

想知道旧代码的某个文件去哪了，查
[`../refactor/test-matrix.md`](../refactor/test-matrix.md) 的「旧位置」列。
