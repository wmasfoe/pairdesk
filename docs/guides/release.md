# 发版流程

> 这份文档回答：**怎么发一个版本，CI 各步骤做什么，出问题怎么补发。**

## 一句话

```bash
git tag v0.2.0
git push origin v0.2.0
```

只有打 tag 才会发版。

## 为什么 push 到 main 不发版

这是**刻意设计**的。早先的流程里 push 到 main 会跑完整构建，结果是：

> 合并 PR 后看到 CI 变绿 → 以为已经发版 → 实际什么都没发布。

现在两个 workflow 分工明确：

| Workflow | 触发 | 做什么 | 产出 |
|---|---|---|---|
| `verify.yml` | push 到 main / PR | 编译 + 单测（三平台） | **无安装包** |
| `release.yml` | 打 tag `v*` / 手动 dispatch | 构建 + 打包 + 发布 | 安装包 + GitHub Release |

**看到 `verify` 绿 ≠ 发版了。** 只有 `release` 跑完才叫发版。

## 发版前的版本号

`release.yml` 的 `Sync version` 步骤会从 tag 提取版本号并写入：

| 文件 | 字段 |
|---|---|
| `crates/*/Cargo.toml` | `version` |
| `crates/pairdesk-ui/packager.toml` | `version`（决定安装包文件名） |
| `crates/pairdesk-ui/Info.plist` | `CFBundleVersion` / `CFBundleShortVersionString` |

因此**不需要手动改版本号**，打 tag 即可。但本地开发时若想让 `cargo test` 里的
`env!("CARGO_PKG_VERSION")` 反映新版本，需要先手动改 Cargo.toml 再打 tag。

## CI 步骤（release.yml）

```
1. Install Linux deps        wayland/xkbcommon/fontconfig/vulkan
2. Install Rust
3. Cache cargo
4. Ensure Metal toolchain     macOS，失败不阻塞（已用 runtime_shaders）
5. Sync version               从 tag 写入所有配置文件
6. Build                      cargo build --release --workspace
7. Unit tests                 cargo test --release --lib --workspace
8. Domain & UI tests          显式点名，避免被 --lib 过滤漏掉
9. Package                    cargo packager，按平台传 --formats
10. Collect CLI binaries
11. Build MUSL static CLI    仅 Linux
12. Upload artifacts
--- 以上三平台并行（ubuntu-22.04 / macos-15 / windows-2022）---
13. publish                   下载 artifacts → gh release create/upload
```

## 产物命名

| 平台 | 文件名 | 大小 |
|---|---|---|
| macOS | `PairDesk_<ver>_aarch64.dmg` + `PairDesk.app` | ~8.8 MB |
| Windows | `pairdesk-ui_<ver>_x64_en-US.msi` | ~7.9 MB |
| Linux | `pairdesk-ui_<ver>_amd64.deb` / `_x86_64.AppImage` | ~14 MB |
| CLI | `pairdesk-<OS>-<arch>` / `pairdesk-relay-<OS>-<arch>` | — |
| CLI 静态 | `pairdesk-linux-x86_64-musl` | — |

> 前缀不同是 cargo-packager 的行为：macOS 用 `product_name`（PairDesk），
> Linux/Windows 用二进制名（pairdesk-ui）。应用内「检查更新」只按扩展名匹配
> （`.dmg` / `.msi` / `.AppImage`），不受前缀影响。

## 打包格式必须与更新逻辑对齐

`Package` 步骤按平台传 `--formats`：

| 平台 | formats | 原因 |
|---|---|---|
| macOS | `app,dmg` | 更新逻辑匹配 `aarch64.dmg` / `x86_64.dmg` |
| Windows | `wix` | 只认 `.msi`；`nsis` 产出的 `setup.exe` 匹配不到 |
| Linux | `deb,appimage` | `default` 会额外产出 pacman 的 `tar.gz` + `PKGBUILD` |

**不要**在 `packager.toml` 里写死 `formats`：它是全局列表，cargo-packager
不会忽略不适用当前平台的格式。实测在 Windows 上写 `app` 会让它真的去创建
`dist/PairDesk.app/Contents/MacOS/`，然后因找不到文件而失败。

## 补发某个 tag

CI 曾失败需要重跑时，用手动 dispatch：

```bash
gh workflow run release.yml -f release_tag=v0.2.0
```

或去 Actions 页面点 `Run workflow` 并填 `release_tag`。

## 已知问题

### macOS 本地打包会在 Finder AppleScript 阶段报错

```
execution error: "Finder"遇到一个错误：不能将"statusbar visible of container window
of disk "PairDesk"""设置为"false" (-10006)
```

这是 cargo-packager 与新版 macOS 的兼容问题，**只在本地出现**，
CI 的 `macos-15` runner 正常。

### 未签名

三个平台的包都是 adhoc / 未签名：
- macOS：`signing_identity = "-"`，安装后需去 quarantine
- Windows：MSI 未签名，SmartScreen 会拦
- Linux：deb 未签名

企业环境可能被策略拦截。
