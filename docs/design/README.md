# 视觉设计索引

> 这个目录回答：**界面长什么样，颜色/字号/间距从哪来。**

## 文件

| 文件 | 内容 |
|---|---|
| `pairdesk-ui-design.html` | 原始交互稿（HTML 原型，可直接浏览器打开） |
| `pd-ui-home.png` | 首页设计稿 |
| `pd-ui-host.png` | 被控端设计稿 |
| `pd-ui-viewer.png` | 控制端设计稿 |
| `logo/cat-signal-1024.png` | Logo 1024px |
| `logo/cat-signal.png` | Logo 展示尺寸 |

## 设计 token 在哪

GPUI 迁移后，设计 token 的**唯一真相**是：

```
crates/pairdesk-ui/src/theme.rs
```

包含：
- `light()` / `dark()` 两套 `Palette`（色值与旧 `styles.css` 逐字一致）
- `size` 模块（圆角、控件高度、字号、间距）
- `font` 模块（字体族名）

旧的 `packages/ui-kit/src/styles.css` 与 `theme.ts` 已不再被 GPUI 版使用，
但迁移期间保留作为色值参照。

## 配色速查

| 语义 | 亮色 | 暗色 |
|---|---|---|
| 页面底色 | `#f4f5f7` | `#0b0d10` |
| 卡片底色 | `#ffffff` | `#13161c` |
| 主文字 | `#111318` | `#eef1f6` |
| 次要文字 | `#4b5563` | `#9aa3b2` |
| 主色（品牌青） | `#0f766e` | `#2dd4bf` |
| 危险色 | `#dc2626` | `#f87171` |
| 状态·在线 | `#34d399` | `#34d399` |
| 状态·连接中 | `#f59e0b` | `#f59e0b` |
| 画面区底色 | `#07080a` | `#07080a` |

## 关键尺寸

| 项 | 值 |
|---|---|
| 窗口 | 980 × 680 |
| 表单最大宽度 | 512px |
| 控件高度 | 40px（凭据框 52px） |
| 圆角 | 6 / 10 / 14px |
| 正文字号 | 15px |
| 凭据字号 | 22px |
