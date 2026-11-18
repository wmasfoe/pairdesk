# 构建与运行

> 这份文档回答：**怎么在本机构建、跑单测、启动 App，三平台各需要什么依赖。**

## 1. 单测

```bash
# 领域层（81 个，秒级，无需图形环境）
cargo test -p pairdesk-shell

# GPUI 适配器层（9 个，含 JPEG 解码性能验证）
cargo test -p pairdesk-ui

# 全 workspace lib 单测（113 个）
cargo test --lib --workspace
```

> 为什么用 `--lib`：`examples/paint` 与 `capture_probe` 是纯 X11 工具（用 `x11rb`
> 直接操作 X server），只在 Linux 上有意义。它们已在 `pairdesk-core/Cargo.toml`
> 里用 `required-features = ["x11-examples"]` 门控，因此 `--all-targets` 在
> macOS/Windows 上会自动跳过它们。

## 构建测试辅助二进制

`tests/` 里的 e2e 脚本需要一个纯色窗口当"被采集的屏幕"（`examples/paint`）。
统一用这个脚本预构建：

```bash
./scripts/build-test-bins.sh          # debug
./scripts/build-test-bins.sh --release
```

在 Linux 上它会额外构建 X11 examples；其他平台自动跳过。

## 2. 构建

```bash
# 调试
cargo build -p pairdesk-ui

# 发布（体积 19MB，RSS 103MB，CPU ~1%）
cargo build --release -p pairdesk-ui
```

## 3. 运行

```bash
# 正常启动（首页）
./target/debug/pairdesk-ui

# 直接进入指定页面（跳过点击，UI 验证用）
PD_START_PAGE=host ./target/debug/pairdesk-ui
PD_START_PAGE=viewer ./target/debug/pairdesk-ui
```

## 4. 端到端自测

三个进程：中继 + 被控端 + 控制端。

```bash
# 1) 起中继
./target/release/pairdesk-relay 8977 &

# 2) 起被控端（自动起会话，跳过 UI 交互）
PD_START_PAGE=host PD_AUTOSTART=host \
  PD_SID=test PD_PASSWORD=123456 PD_RELAY=127.0.0.1:8977 \
  ./target/release/pairdesk-ui &

# 3) 起控制端（自动连入）
PD_START_PAGE=viewer PD_AUTOSTART=viewer \
  PD_SID=test PD_PASSWORD=123456 PD_RELAY=127.0.0.1:8977 \
  ./target/release/pairdesk-ui &
```

预期日志（被控端）：

```
[host-auto] ✅ 双向打洞穿透成功，切换为 QUIC 直连链路！
[host-session] ✅ 密码认证成功，发送 AuthOk，建立加密通道…
```

预期日志（控制端）：

```
[viewer-auto] ✅ 双向打洞穿透成功，切换为 QUIC 直连链路！
[viewer-session] ✅ 认证通过，会话加密建立成功！
```

界面应显示会话码 + `QUIC 打洞直连` + 实时画面。

### 环境变量一览

| 变量 | 作用 |
|---|---|
| `PD_START_PAGE` | `host` / `viewer`，启动直达页面 |
| `PD_AUTOSTART` | `host` / `viewer`，跳过 UI 直接起会话 |
| `PD_SID` | 会话码 |
| `PD_PASSWORD` | 连接密码 |
| `PD_RELAY` | 中继地址（`IP:端口`） |
| `PD_HOLE` | 打洞端口 |

## 5. 三平台系统依赖

### macOS

```bash
xcode-select --install
```

若遇到：

```
error: cannot execute tool 'metal' due to missing Metal Toolchain
```

两个解法（二选一）：

```bash
# A. 装 Metal Toolchain（CI 推荐）
xcodebuild -downloadComponent MetalToolchain
```

```toml
# B. 用运行时编译 shader（当前 Cargo.toml 已启用）
gpui = { version = "0.2.2", features = ["runtime_shaders"] }
```

### Linux

```bash
sudo apt-get install -y \
  libwayland-dev libxkbcommon-dev libfontconfig-dev \
  libx11-dev libxkbcommon-x11-dev \
  mesa-vulkan-drivers vulkan-tools
```

**注意**：GPUI 的 Linux 渲染走 Vulkan，**没有 Vulkan 驱动的环境起不来**（虚拟机、老显卡）。
验证：

```bash
vulkaninfo --summary   # 应能看到设备
```

### Windows

- MSVC toolchain（`rustup default stable-msvc`）
- Windows SDK（提供 `fxc.exe`）
- 需要 D3D11 Feature Level 11.0+

release 构建 `build.rs` 会调 `fxc.exe` 编译 shader，找不到直接 panic。
可设环境变量指定路径：

```powershell
$env:GPUI_FXC_PATH = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\fxc.exe"
```

## 6. 打包

```bash
# 一次性安装工具
cargo install cargo-packager --locked

# 产出安装包（macOS: .app + .dmg；Linux: .deb + .AppImage；Windows: .msi + .exe）
cargo packager -p pairdesk-ui --config crates/pairdesk-ui/packager.toml
```

产物落在仓库根的 `dist/`（已 gitignore）。

### 开发时不要安装 dmg

**日常验证一律直接跑 dev 构建的二进制**，不要挂载/安装 dmg：

```bash
cargo build -p pairdesk-ui && ./target/debug/pairdesk-ui
```

原因：
- 装到 `/Applications` 后，系统的屏幕录制/辅助功能授权会绑到那个路径，
  而开发迭代时二进制一直在变，权限会反复失效，排查起来很绕。
- 未签名包每次替换都要处理 quarantine。

只有**验证最终分发包**时才临时挂载 dmg（不要拖进 `/Applications`）：

```bash
hdiutil attach -nobrowse dist/PairDesk_*_aarch64.dmg
# 直接运行挂载点里的二进制验证，验证完：
hdiutil detach /Volumes/PairDesk
```

### macOS 权限声明

`crates/pairdesk-ui/Info.plist` 里声明了两个用途描述（缺失会导致系统静默拒绝授权）：

| 键 | 用途 |
|---|---|
| `NSScreenCaptureUsageDescription` | 被控端采集屏幕 |
| `NSAppleEventsUsageDescription` | 控制端注入输入 |

## 7. 已知构建坑

| 现象 | 原因 | 解法 |
|---|---|---|
| `missing Metal Toolchain` | Xcode 27 拆出独立组件 | 装组件 或 用 `runtime_shaders` |
| `div().on_click` 找不到方法 | `on_click` 在 `StatefulInteractiveElement` 上 | 先 `.id("...")` |
| `rgb()` 不能用于 `const` | 非 const fn | 主题改用函数返回 |
| Linux 起不来窗口 | 无 Vulkan 驱动 | 装 `mesa-vulkan-drivers` |
| 中文显示为方块（Linux） | GPUI 字体回退弱 | 内置中文字体子集 |
