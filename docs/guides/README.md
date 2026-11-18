# 开发指南索引

> 这个目录回答：**怎么跑、怎么构建、怎么发版。**

## 文件

| 文件 | 回答什么问题 |
|---|---|
| [`build.md`](build.md) | 本地构建、单测、跑起来、三平台依赖 |
| [`relay-deploy.md`](relay-deploy.md) | 中继服务器部署到 VPS |

## 快速命令

```bash
# 领域层单测（81 个，秒级）—— 改业务逻辑后必跑
cargo test -p pairdesk-shell

# 全 workspace 单测（113 个）
cargo test --lib --workspace

# 构建 GPUI 壳
cargo build -p pairdesk-ui

# 跑起来
./target/debug/pairdesk-ui
```

## 三平台系统依赖速查

| 平台 | 依赖 |
|---|---|
| macOS | Xcode（或仅 CLT + `runtime_shaders` feature） |
| Linux | `libwayland-dev` `libxkbcommon-dev` `libfontconfig-dev` `libx11-dev` + Vulkan 驱动 |
| Windows | MSVC toolchain + Windows SDK（含 `fxc.exe`） |

详见 [`build.md`](build.md)。
