# 重构进度

> 这份文档回答：**现在做到哪了，下一步做什么。**

**最后更新**：完成 macOS 端到端验证

## 总览

| 阶段 | 状态 |
|---|---|
| 1. 抽出领域层 `pairdesk-shell` | ✅ 完成（81 个单测） |
| 2. 建立文档索引体系 | ✅ 完成 |
| 3. GPUI 壳 `pairdesk-ui` 骨架 | ✅ 完成 |
| 4. 三页面渲染（首页/被控端/控制端） | ✅ 完成 |
| 5. 端到端连接（macOS） | ✅ 完成 |
| 6. 输入注入（鼠标/键盘/滚轮） | ✅ 完成（实测 104+ 次注入成功） |
| 7. 权限引导 UI | ✅ 完成（打开设置 / 重新检测 / 重启应用） |
| 8. 更新检查 UI | ✅ 完成（弹窗 + 前往下载） |
| 9. Linux 编译验证 | 🟡 领域层通过；壳需 CI 真 runner（缺系统库） |
| 10. Windows 编译验证 | 🟡 领域层通过；壳需 CI 真 runner（缺 fxc.exe） |
| 11. 打包配置（.dmg/.msi/.AppImage） | ✅ 完成（.dmg 8.8MB 实测产出） |
| 12. 删除 Tauri 遗留 | ✅ 完成（105 个文件已移除，仓库只剩 Rust） |

## 实测数据（macOS / Apple Silicon / release）

| 指标 | GPUI 版 | Tauri 版（对比） |
|---|---|---|
| 二进制体积 | **19 MB** | 35 MB（debug）+ 372KB 前端产物 |
| 稳态 RSS | **103 MB** | — |
| 稳态 CPU | **~1%** | — |
| 纹理释放 | 6 次采样 RSS 零增长 ✅ | — |
| 连接建立 | STUN → 中继信令 → QUIC 打洞 → 认证，全链路 ✅ | ✅ |
| 画面渲染 | 实时 20fps ✅ | ✅ |
| 错误态渲染 | 红色错误条正确显示 ✅ | ✅ |

## 已完成的工作

### 领域层 `crates/pairdesk-shell`（81 个单测）

| 模块 | 迁移来源 | 单测数 |
|---|---|---|
| `session.rs` | `useSession.ts` + `bridge.rs` | 24 |
| `input.rs` | `VideoView.tsx` | 22 |
| `credentials.rs` | `HostPage.tsx` | 6 |
| `permissions.rs` | `PermissionBanner.tsx` + `usePermissions.ts` | 13 |
| `update.rs` | `bridge.rs::pd_check_update` | 15 |

### 适配器层 `crates/pairdesk-ui`（新增）

| 文件 | 职责 |
|---|---|
| `main.rs` | 入口：初始化 gpui-component、开窗口、包 Root |
| `theme.rs` | 设计 token（亮/暗两套，色值与旧 CSS 一致） |
| `frame.rs` | JPEG 解码 + 帧泵 + 纹理释放（含 6 个单测） |
| `session_host.rs` | 意图 → `CoreHandle` 调用翻译（含 3 个单测） |
| `app_view.rs` | 三页面渲染 + 事件转发 + 更新弹窗 |
| `video_view.rs` | 画面区：渲染 + 鼠标键盘滚轮捕获（含 6 个单测） |
| `update.rs` | GitHub API 请求（纯计算在 shell） |

### 关键问题修复

1. **帧率塌陷**：在 `render()` 里 `cx.notify()` 导致 0.2fps。改用 `cx.spawn` + `background_executor().timer()` 后恢复 20fps。
2. **纹理泄漏**：每帧必须 `cx.drop_image()` 释放旧纹理，否则 GPU atlas 无限增长。已验证 RSS 稳定。
3. **macOS 构建失败**：缺 Metal Toolchain。用 `features = ["runtime_shaders"]` 绕过。
4. **`div().on_click()` 编译失败**：必须先 `.id()` 得到 `Stateful<Div>`。
5. **`rgb()` 不是 const fn**：主题不能用 `const`，改为函数。

## 待办

### P0（已完成 ✅）

- [x] **输入注入接线**：`video_view.rs` 捕获鼠标移动/按下/松开/滚轮/键盘，经 `pairdesk_shell::input` 换算后发出。
- [x] **权限引导按钮**：打开系统设置 / 重新检测 / 重启应用。
- [x] **复制按钮**：凭据行与「复制会话信息」均可用。

### P1（跨平台）

- [x] **CI workflow 改造**：去掉 webkit 依赖，换成 wayland/xkbcommon/fontconfig/vulkan，新增 `Build GPUI shell` + 单测步骤
- [x] **交叉编译验证**：`cargo zigbuild` 实测 —— `pairdesk-shell` 在 Linux 与 Windows 均编译通过（证明领域层零平台依赖）
- [ ] Linux 编译验证（**需 CI 真 runner**：本机缺 `xcb`/`xkbcommon` 系统库）
- [ ] Windows 编译验证（**需 CI 真 runner**：本机缺 `fxc.exe`）
- [ ] Linux 中文字体内置（GPUI 的 Linux 字体回退是已知弱项）

### P2（收尾）

- [x] **打包**：`cargo-packager` 配置完成，实测产出 `PairDesk.app` + `PairDesk_0.1.21_aarch64.dmg`（8.8MB）
- [x] **权限声明**：`Info.plist` 声明屏幕录制/AppleEvents 用途（缺失会导致系统静默拒绝授权）
- [x] **删除 Tauri 遗留**：`app/`、`packages/`、`crates/pairdesk-app/`、`package.json`、
      `pnpm-lock.yaml`、`pnpm-workspace.yaml`、`tsconfig.base.json`、3 个 Tauri 专用脚本
- [x] **更新根 `README.md`**：技术栈改为 Rust + GPUI，形态改为四层架构
- [x] **CI 改造**：去掉全部 pnpm/tauri 步骤，改为 cargo build + cargo packager
- [x] **`tests/app-e2e.sh` 重写**：从 Tauri 的 `PD_ROLE` 改为 GPUI 的 `PD_AUTOSTART`，实测 5/5 通过
- [ ] 迁移 UI 交互测试为 GPUI 的 `test-support`（原 Playwright 方案已随前端删除）

## 功能对齐核对

旧 `bridge.rs` 的 14 个 Tauri 命令全部有对应实现：

| 旧命令 | GPUI 版 | 状态 |
|---|---|---|
| `pd_set_allowed` | `toggle_allowed` → `session.set_allowed` | ✅ |
| `pd_start_host` / `_auto` | `host_start` → `dispatch(StartHost)` | ✅ |
| `pd_connect` / `_auto` | `viewer_connect` → `dispatch(Connect)` | ✅ |
| `pd_stop` | `host_disconnect` / `viewer_disconnect` | ✅ |
| `pd_send_input` | `video_view` → `session.send_input` | ✅ |
| `pd_check_permissions` | `permissions::check_permissions` | ✅ |
| `pd_request_permission` | 权限横幅「请求授权」 | ✅ |
| `pd_open_permission_settings` | 权限横幅「打开系统设置」 | ✅ |
| `pd_check_update` | `update::check` + 弹窗 | ✅ |
| `pd_install_update` | `update::download_installer` + `install_and_exit` | ✅ |
| `pd_open_url` | `cx.open_url` | ✅ |
| `pd_restart_app` | 权限横幅「重启应用」 | ✅ |

## 自测命令

```bash
# 领域层单测（81 个，秒级）
cargo test -p pairdesk-shell

# 全 workspace 单测（104 个）
cargo test --lib --workspace

# 编译
cargo build -p pairdesk-ui

# 手动跑首页
./target/debug/pairdesk-ui

# 手动跑指定页面（跳过点击）
PD_START_PAGE=host ./target/debug/pairdesk-ui
PD_START_PAGE=viewer ./target/debug/pairdesk-ui

# 端到端（两个进程 + relay）
./target/release/pairdesk-relay 8977 &
PD_START_PAGE=host PD_AUTOSTART=host PD_SID=test PD_PASSWORD=123456 \
  PD_RELAY=127.0.0.1:8977 ./target/release/pairdesk-ui &
PD_START_PAGE=viewer PD_AUTOSTART=viewer PD_SID=test PD_PASSWORD=123456 \
  PD_RELAY=127.0.0.1:8977 ./target/release/pairdesk-ui &
```
