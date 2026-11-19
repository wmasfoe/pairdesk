# PairDesk — Agent 工作约定

> 本文件被 agent 自动加载。**开始任何任务前先读这里。**

## 查文档：先读索引，再读文件

`docs/` 采用「索引驱动」结构。**不要 `find docs` 或 `grep -r docs`**，那会读到过期文档并浪费上下文。

正确顺序：

```
1. 读 docs/README.md          ← 总索引，按任务类型找到子目录
2. 读 <子目录>/README.md       ← 该目录的文件清单与用途
3. 打开具体文件
```

| 任务 | 入口 |
|---|---|
| 理解分层与依赖方向 | `docs/architecture/README.md` |
| 重构规格与进度 | `docs/refactor/README.md` |
| 构建/运行/发版 | `docs/guides/README.md` |
| 配色与尺寸 | `docs/design/README.md` |
| 旧 Tauri 实现 | `docs/legacy/README.md`（已过期，仅考古） |

**新增文档时**：必须同步更新所在目录的 `README.md` 和 `docs/README.md` 的「按任务查找」表。

## 架构铁律

```
pairdesk-ui (GPUI 壳)  ──→  pairdesk-shell (领域)  ──→  pairdesk-core (引擎)
   只画 + 只转发              业务规则 + 单测            系统 API + 网络
```

写代码前问：**这段逻辑能在没有窗口的情况下被单测验证吗？**

- 能 → 放 `pairdesk-shell`（业务）或 `pairdesk-core`（系统/网络）
- 不能（必须画出来才知道对不对）→ 才放 UI 层，且**只做渲染与事件转发**

UI 层**禁止**：会话阶段推导、坐标换算、按键映射、权限判定、凭据格式。
这些都在 `pairdesk-shell`，有单测覆盖。详见 `docs/architecture/clean-architecture.md`。

## 改代码后的验证

```bash
cargo test -p pairdesk-shell      # 88 个领域单测，秒级
cargo test --lib --workspace      # 全量 113 个
cargo build -p pairdesk-ui        # 编译
```

**业务逻辑改动必须让 `pairdesk-shell` 的单测保持全绿**——那是行为等价的护栏。

## 已知的 GPUI 坑（不要重新踩）

| 坑 | 正确做法 |
|---|---|
| **在 `cx.spawn` 里用 tokio 生态的异步库（reqwest/hyper/tokio::net）** | **App 直接崩溃退出**。GPUI 的 executor 是 `async-task`+`smol`，不是 tokio。改用 `reqwest::blocking` + 独立线程 + `futures-timer` 等待 |
| 在 `render()` 里 `cx.notify()` 拉帧 | 用 `cx.spawn` + `background_executor().timer()`，否则帧率塌到 0.2fps |
| 忘了释放旧纹理 | 每帧 `cx.drop_image(old, None)`，否则 GPU 内存无限增长 |
| `div().on_click()` 编译失败 | 先 `.id("...")` 得到 `Stateful<Div>` |
| `rgb()` 用在 `const` 里 | 主题改用函数返回 `Palette` |
| macOS 缺 Metal Toolchain | `gpui = { features = ["runtime_shaders"] }` |

详见 `docs/architecture/gpui-constraints.md`。

## 开发时怎么跑

**一律直接跑 dev 二进制，不要安装 dmg / 拖进 `/Applications`：**

```bash
cargo build -p pairdesk-ui && ./target/debug/pairdesk-ui
```

装到 `/Applications` 会让 macOS 的屏幕录制/辅助功能授权绑到那个路径，
开发迭代时二进制一直在变，权限会反复失效。只有验证最终分发包时才临时挂载 dmg。

## 版本锁定

`gpui = "0.2.2"` + `gpui-component = "0.5.0"`。

**不要升级**：0.6+ 改依赖 `gpui-pre`，0.10 需要 nightly，且 nightly 上编不过。
理由见 `docs/architecture/gpui-constraints.md` 第 1 节。

## 代码风格

- 注释与文档用**中文**，与现有代码一致
- 单测函数名用中文（`密码恒为六位数字`），读起来就是规格
- 每个公开函数写清「为什么」而不是「做什么」
