# GPUI 技术约束与已踩过的坑

> 这份文档回答：**GPUI 有什么限制，哪些坑已经踩过，怎么绕。**

所有结论均在本机（macOS 26 / Xcode 27 / rustc 1.96）实测得出，非文档推测。

## 1. 版本锁定

| 包 | 锁定版本 | 原因 |
|---|---|---|
| `gpui` | **0.2.2** | 最后一个走 stable Rust 的正式发布 |
| `gpui-component` | **0.5.x** | 最后一个依赖 `gpui 0.2.2` 的版本 |

**不要升级**：

- `gpui-component >= 0.6` 改为依赖 `gpui-pre`（Zed 源码快照），与 `gpui 0.2.2` 类型不兼容。
- `gpui-component 0.10` + `gpui-pre 0.3.12` 需要 **nightly**（用了未稳定的 `Option::map_or_default`）。
- 实测 nightly 1.102 上 `gpui-pre` 又会撞 `pathfinder_geometry 0.5.1` 的 monomorphization panic。

结论：**锁 0.2.2 + 0.5.x，不追新。**

## 2. macOS：必须用 runtime_shaders

**症状**：

```
error: cannot execute tool 'metal' due to missing Metal Toolchain;
use: xcodebuild -downloadComponent MetalToolchain
```

即使装了完整 Xcode 27 也会出现——新版 Xcode 把 Metal 编译器拆成独立组件，默认不装。

**两个解法**：

```toml
# 方案 A（推荐，已验证）：让 shader 在运行时编译
gpui = { version = "0.2.2", features = ["runtime_shaders"] }
```

```bash
# 方案 B：CI / 开发机装 Metal Toolchain
xcodebuild -downloadComponent MetalToolchain
```

CI 的 `macos-15` runner 同样会撞，**必须在 workflow 里二选一**。

## 3. 帧泵：不要在 render() 里 cx.notify()

**这是本项目最关键的坑。** 远程画面需要 20fps 刷新。

❌ 错误写法（实测帧率塌到 **0.2 fps**，20 秒只出 4 帧）：

```rust
fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if let Ok(jpeg) = self.rx.try_recv() {
        self.set_frame(jpeg);
        cx.notify();   // ← 在这里 notify 会被合并/丢弃
    }
    div()
}
```

✅ 正确写法（实测稳定 **18–20 fps**）：

```rust
fn pump(&mut self, cx: &mut Context<Self>) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(Duration::from_millis(15)).await;
            let ok = this.update(cx, |v, cx| {
                let Ok(jpeg) = v.rx.try_recv() else { return };
                v.set_frame(jpeg);   // 内部调 cx.drop_image 释放旧纹理
                cx.notify();
            });
            if ok.is_err() { break; }
        }
    }).detach();
}
```

## 4. 纹理必须显式释放

每帧 `RenderImage::new()` 都会在 GPU atlas 里占一块。**不释放会无限增长**直到显存耗尽。

```rust
if let Some(old) = self.frame.replace(new_frame) {
    cx.drop_image(old, None);   // ← 必须
}
```

实测：正确释放时 RSS 稳定在 **129 MB**（连续 30 秒 20fps 1080p 不涨）。

## 5. 图片必须是 BGRA

`RenderImage` 要求 `image::Frame` 包裹的 **RGBA8** 数据，GPUI 内部按 BGRA 解释。从 JPEG 解码后需要：

```rust
let rgb = zune_jpeg::JpegDecoder::new(&jpeg[..]).decode()?;
let mut rgba = Vec::with_capacity(w * h * 4);
for c in rgb.chunks_exact(3) {
    rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);   // RGB → RGBA
}
let frame = Frame::new(image::RgbaImage::from_raw(w, h, rgba)?);
```

## 6. 性能实测基线

| 指标 | 实测值 | 备注 |
|---|---|---|
| 1080p JPEG 解码（`zune-jpeg`） | **3.4 ms/帧** | 纯解码 |
| 1080p JPEG 解码（`image` crate） | 6.4 ms/帧 | 慢一倍，不推荐 |
| RGB → RGBA 转换 | < 0.1 ms/帧 | 可忽略 |
| 20fps 稳态 CPU | **17–23%** | 单核百分比 |
| 稳态 RSS | **129 MB** | 1080p 单帧纹理 |
| release 二进制 | **15 MB** | 含 gpui-component |

20fps 预算 50ms/帧，解码占 3.4ms ≈ 7%，**余量充足**。

## 7. 组件库能力（gpui-component 0.5.x）

可直接用：`Button`、`Input`（含 IME/选区/剪贴板）、`Switch`、`Root`、`Text`、`Icon`、`Tab`、`Modal`。

**没有的**：
- 原生 `<select>` 下拉 → 用 `popover` + `list` 手搭
- `Disclosure`（折叠面板）→ 手写（一个 bool + `when()` 即可）
- 剪贴板 → `cx.write_to_clipboard()`
- 打开 URL → `cx.open_url()`

## 8. 平台后端成熟度

| 平台 | 窗口 | GPU | 成熟度 |
|---|---|---|---|
| macOS | AppKit | Metal | ✅ 高（Zed 主力平台） |
| Linux | Wayland / X11 | Vulkan（blade） | 🟡 中 |
| Windows | Win32 | Direct3D 11 | 🟡 中低 |

**Windows 注意事项**：
- `gpui 0.2.2` 官方 README 只声明支持 macOS/Linux，Windows 代码存在但未在文档承诺。
- 代码里有 3 处 `unimplemented!()`（`hide_other_apps`、`unhide_other_apps`、`display_handle`），均不在关键路径。
- **release 构建 `build.rs` 硬依赖 `fxc.exe`**（`panic!("Failed to find fxc.exe")`），需 Windows SDK；debug 构建走 `D3DCompileFromFile` 运行时编译可绕过。
- 需要 D3D11 Feature Level 11.0+。

### 交叉编译验证结果（macOS → Linux/Windows）

用 `cargo zigbuild` 实测（`zig 0.17.1` + `cargo-zigbuild 0.24.1`）：

| 目标 | `pairdesk-shell` | `pairdesk-ui` | 失败原因 |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ 编译通过 | ⚠️ 链接失败 | 缺 `xcb` / `xkbcommon` / `xkbcommon-x11` 系统库 |
| `x86_64-pc-windows-gnu` | ✅ 编译通过 | ❌ 编译失败 | `gpui` build.rs 需要 `fxc.exe`（Windows SDK） |

**结论**：
- 领域层代码**零平台依赖**，三平台都能编过 —— 这验证了整洁架构的隔离是有效的。
- GPUI 壳的跨平台验证**必须在真实 runner 上做**，macOS 交叉编译无法替代。
- 这也是 CI 里加 `Build GPUI shell` 步骤的原因：只有真 runner 才有对应的系统库/SDK。

**Linux 注意事项**：
- 渲染走 Vulkan，**无 Vulkan 驱动的环境起不来**（虚拟机、老显卡）。
- 中文字体回退是已知弱项，源码里有 `// HACK: ... should actually do better font fallback`，**建议内置中文子集字体**。

## 9. 测试

`gpui` 的 `test-support` feature 提供 `TestAppContext` + `simulate_*`，可无头驱动 UI。

但**业务逻辑不应依赖它验证**——那部分必须在 `pairdesk-shell` 里用纯单测覆盖。GPUI 测试只用于验证"渲染没崩、点击能响应"。
