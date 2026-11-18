//! 设计 token —— 从 `packages/ui-kit/src/theme.ts` 与 `styles.css` 迁移。
//!
//! GPUI 没有 CSS 变量，颜色必须是编译期常量。这里保留**双主题**结构：
//! 每个 token 给出亮/暗两套色值，运行时按系统外观取一套。
//!
//! 色值与旧 CSS 变量逐字对应，保证迁移后视觉一致。
//!
//! 部分 token 当前 UI 未用到（如 `primary_hover`、`gap_xl`），但保留完整的设计
//! 系统比"用到什么写什么"更可维护——加新界面时不必回头补色值。
#![allow(dead_code)]

use gpui::{rgb, Rgba};

/// 主题模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    /// 跟随系统外观。
    pub fn from_system(dark: bool) -> Self {
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }
}

/// 一套完整配色。
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// 页面底色
    pub bg: Rgba,
    /// 卡片/浮层底色
    pub bg_elev: Rgba,
    /// 悬停底色
    pub bg_hover: Rgba,
    /// 输入框底色
    pub bg_input: Rgba,
    /// 主前景色
    pub fg: Rgba,
    /// 次要文字色
    pub fg_muted: Rgba,
    /// 边框
    pub border: Rgba,
    /// 强边框
    pub border_strong: Rgba,
    /// 主色（品牌青）
    pub primary: Rgba,
    pub primary_hover: Rgba,
    pub primary_pressed: Rgba,
    /// 主色上的文字
    pub primary_fg: Rgba,
    /// 主色淡底
    pub primary_soft: Rgba,
    /// 危险色
    pub danger: Rgba,
    pub danger_hover: Rgba,
    pub danger_fg: Rgba,
    /// 警告色（权限横幅）
    pub warn: Rgba,
    /// 状态色
    pub status_idle: Rgba,
    pub status_connecting: Rgba,
    pub status_online: Rgba,
    pub status_error: Rgba,
    /// 画面区底色（近黑）
    pub video_bg: Rgba,
}

/// 亮色主题。色值来自 `styles.css` 的 `.pd-light` 块。
pub fn light() -> Palette {
    Palette {
    bg: rgb(0xf4f5f7),
    bg_elev: rgb(0xffffff),
    bg_hover: rgb(0xeef0f4),
    bg_input: rgb(0xeef0f4),
    fg: rgb(0x111318),
    fg_muted: rgb(0x4b5563),
    border: rgb(0xd5dae3),
    border_strong: rgb(0xc3c9d4),
    primary: rgb(0x0f766e),
    primary_hover: rgb(0x0d9488),
    primary_pressed: rgb(0x115e59),
    primary_fg: rgb(0xffffff),
    primary_soft: gpui::rgba(0x0f766e1f),
    danger: rgb(0xdc2626),
    danger_hover: rgb(0xb91c1c),
    danger_fg: rgb(0xffffff),
    warn: rgb(0xf59e0b),
    status_idle: rgb(0x4b5563),
    status_connecting: rgb(0xf59e0b),
    status_online: rgb(0x34d399),
    status_error: rgb(0xdc2626),
    video_bg: rgb(0x07080a),
    }
}

/// 暗色主题。色值来自 `styles.css` 的 `.pd-dark` / `prefers-color-scheme: dark` 块。
pub fn dark() -> Palette {
    Palette {
    bg: rgb(0x0b0d10),
    bg_elev: rgb(0x13161c),
    bg_hover: rgb(0x1a1f27),
    bg_input: rgb(0x0e1116),
    fg: rgb(0xeef1f6),
    fg_muted: rgb(0x9aa3b2),
    border: gpui::rgba(0xffffff14),      // rgba(255,255,255,0.08)
    border_strong: gpui::rgba(0xffffff24), // rgba(255,255,255,0.14)
    primary: rgb(0x2dd4bf),
    primary_hover: rgb(0x5eead4),
    primary_pressed: rgb(0x14b8a6),
    primary_fg: rgb(0x042f2e),
    primary_soft: gpui::rgba(0x2dd4bf1f), // rgba(45,212,191,0.12)
    danger: rgb(0xf87171),
    danger_hover: rgb(0xfca5a5),
    danger_fg: rgb(0x1f0a0a),
    warn: rgb(0xf59e0b),
    status_idle: rgb(0x6b7280),
    status_connecting: rgb(0xf59e0b),
    status_online: rgb(0x34d399),
    status_error: rgb(0xf87171),
    video_bg: rgb(0x07080a),
    }
}

impl Palette {
    /// 按模式取配色。
    pub fn of(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => light(),
            ThemeMode::Dark => dark(),
        }
    }

    /// 状态色调 → 颜色。对应旧 `STATUS_COLORS` 表。
    pub fn status(&self, tone: pairdesk_shell::StatusTone) -> Rgba {
        use pairdesk_shell::StatusTone;
        match tone {
            StatusTone::Idle => self.status_idle,
            StatusTone::Connecting => self.status_connecting,
            StatusTone::Online => self.status_online,
            StatusTone::Error => self.status_error,
        }
    }
}

/// 尺寸 token。对应旧 `SIZE` 常量与 CSS 里的 rem 值（1rem = 16px）。
pub mod size {
    use gpui::{px, Pixels};

    /// 圆角
    pub fn radius_sm() -> Pixels {
        px(6.)
    }
    pub fn radius_md() -> Pixels {
        px(10.)
    }
    pub fn radius_lg() -> Pixels {
        px(14.)
    }

    /// 控件高度
    pub fn control() -> Pixels {
        px(40.)
    }
    /// 大号控件高度（凭据输入框）
    pub fn control_lg() -> Pixels {
        px(52.)
    }

    /// 字号
    pub fn text_sm() -> Pixels {
        px(13.)
    }
    pub fn text_md() -> Pixels {
        px(15.)
    }
    pub fn text_lg() -> Pixels {
        px(18.)
    }
    pub fn text_xl() -> Pixels {
        px(24.)
    }
    /// 凭据大字
    pub fn text_credential() -> Pixels {
        px(22.)
    }
    /// 密码展示字
    pub fn text_password() -> Pixels {
        px(22.)
    }

    /// 间距
    pub fn gap_xs() -> Pixels {
        px(4.)
    }
    pub fn gap_sm() -> Pixels {
        px(8.)
    }
    pub fn gap_md() -> Pixels {
        px(12.)
    }
    pub fn gap_lg() -> Pixels {
        px(16.)
    }
    pub fn gap_xl() -> Pixels {
        px(24.)
    }

    /// 页面内边距
    pub fn page_pad() -> Pixels {
        px(24.)
    }

    /// 窗口默认尺寸
    pub fn window_width() -> f32 {
        980.
    }
    pub fn window_height() -> f32 {
        680.
    }

    /// 表单最大宽度
    pub fn form_max_width() -> Pixels {
        px(512.)
    }
}

/// 字体族名。与旧 CSS 的 `--pd-font` 系列一致。
pub mod font {
    /// 正文字体
    pub const SANS: &str = "SF Pro Text";
    /// 标题字体
    pub const DISPLAY: &str = "SF Pro Display";
    /// 等宽字体（会话码/密码）
    pub const MONO: &str = "SF Mono";
}
