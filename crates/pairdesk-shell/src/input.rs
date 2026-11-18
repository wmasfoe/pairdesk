//! 输入事件的换算与映射。
//!
//! 行为来源：`app/src/components/VideoView.tsx`。
//!
//! 这里包含三块**纯计算**逻辑，全部从 React 组件里抽出来：
//!
//! 1. 鼠标坐标换算：控制端窗口内的像素 → 远端屏幕真实分辨率坐标
//! 2. 按键映射：浏览器 `KeyboardEvent.code`/`key` → X11 keysym（内核协议使用）
//! 3. 修饰键位：`shiftKey`/`ctrlKey`/... → 位掩码
//!
//! 抽出来的理由：这些公式一旦写错，用户会感到"鼠标点不准"或"按键乱跳"，
//! 而它们完全是纯函数，没有任何理由留在必须起窗口才能测的地方。

use pairdesk_core::protocol::InputMsg as CoreInputMsg;

/// 鼠标按键编号（与 `pairdesk-core::input` 的常量一致）。
pub const BTN_LEFT: u8 = 1;
pub const BTN_MIDDLE: u8 = 2;
pub const BTN_RIGHT: u8 = 3;

/// 修饰键位掩码（与 `pairdesk-core::input` 一致）。
pub const MOD_SHIFT: u32 = 1;
pub const MOD_CTRL: u32 = 2;
pub const MOD_ALT: u32 = 4;
pub const MOD_META: u32 = 8;

/// 无法映射的按键统一返回 0，表示"不发送"。
///
/// 原实现注释：*"其他非 ASCII 字符或 IME 组合中过程键，返回 0 忽略，
/// 避免产生脏键码破坏接收端"*。
pub const KEYSYM_NONE: u32 = 0;

/// 控制端画面上用于坐标换算的矩形。
///
/// 对应 `VideoView.tsx::getRemoteCoord` 里 `img.getBoundingClientRect()` 的结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseRect {
    /// 画面在窗口内的左上角 x
    pub left: f32,
    /// 画面在窗口内的左上角 y
    pub top: f32,
    /// 画面渲染宽度（可能被缩放）
    pub width: f32,
    /// 画面渲染高度
    pub height: f32,
}

impl MouseRect {
    /// 构造并校验：宽高必须为正，否则无法换算。
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Option<Self> {
        if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
            return None;
        }
        Some(Self {
            left,
            top,
            width,
            height,
        })
    }

    /// 把窗口内坐标换算为远端屏幕坐标。
    ///
    /// 原实现：
    /// ```js
    /// const relX = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    /// const relY = Math.max(0, Math.min(e.clientY - rect.top, rect.height));
    /// const x = (relX / rect.width) * targetW;
    /// const y = (relY / rect.height) * targetH;
    /// ```
    ///
    /// 关键语义：**先把相对坐标夹紧到画面矩形内**，再按比例放大。
    /// 夹紧保证鼠标拖到画面外时不会产生越界坐标（远端会点到屏幕外）。
    pub fn to_remote(
        &self,
        client_x: f32,
        client_y: f32,
        remote_w: f32,
        remote_h: f32,
    ) -> (f64, f64) {
        let rel_x = (client_x - self.left).clamp(0.0, self.width);
        let rel_y = (client_y - self.top).clamp(0.0, self.height);
        let x = (rel_x / self.width) * remote_w;
        let y = (rel_y / self.height) * remote_h;
        (f64::from(x), f64::from(y))
    }
}

/// 浏览器鼠标按键 → PairDesk 协议按键号。
///
/// 原实现：
/// ```js
/// const btn = e.button === 0 ? 1 : e.button === 1 ? 2 : e.button === 2 ? 3 : 1;
/// ```
/// 即 0/左→1，1/中→2，2/右→3，其他一律当左键。
pub fn map_mouse_button(browser_button: u8) -> u8 {
    match browser_button {
        1 => BTN_MIDDLE,
        2 => BTN_RIGHT,
        // 0（左键）与任何未知值都当左键，与旧实现一致
        _ => BTN_LEFT,
    }
}

/// 修饰键状态 → 位掩码。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    /// 打包成内核协议的位掩码。
    ///
    /// 原实现：
    /// ```js
    /// let mods = 0;
    /// if (e.shiftKey) mods |= 1;
    /// if (e.ctrlKey)  mods |= 2;
    /// if (e.altKey)   mods |= 4;
    /// if (e.metaKey)  mods |= 8;
    /// ```
    pub fn bits(self) -> u32 {
        let mut m = 0;
        if self.shift {
            m |= MOD_SHIFT;
        }
        if self.ctrl {
            m |= MOD_CTRL;
        }
        if self.alt {
            m |= MOD_ALT;
        }
        if self.meta {
            m |= MOD_META;
        }
        m
    }
}

/// 按键 → X11 keysym。
///
/// 迁移来源：`VideoView.tsx::keyToSym`。
///
/// `code` 是物理键位（如 `"KeyA"`、`"ArrowLeft"`），`key` 是实际字符（受布局和
/// 修饰键影响，如 Shift+a → `"A"`）。优先按 `code` 查特殊键，再回落到 `key`
/// 的单字符 ASCII。
pub fn key_to_sym(code: &str, key: &str) -> u32 {
    // 1) 特殊键与导航键（X11 keysym 常量）
    let special = match code {
        "Backspace" => Some(0xff08),
        "Tab" => Some(0xff09),
        "Enter" | "NumpadEnter" => Some(0xff0d),
        "Escape" => Some(0xff1b),
        "Delete" => Some(0xffff),
        "ArrowLeft" => Some(0xff51),
        "ArrowUp" => Some(0xff52),
        "ArrowRight" => Some(0xff53),
        "ArrowDown" => Some(0xff54),
        "Home" => Some(0xff50),
        "End" => Some(0xff57),
        "PageUp" => Some(0xff55),
        "PageDown" => Some(0xff56),
        "ShiftLeft" | "ShiftRight" => Some(0xffe1),
        "ControlLeft" | "ControlRight" => Some(0xffe3),
        "AltLeft" | "AltRight" => Some(0xffe9),
        "MetaLeft" | "MetaRight" => Some(0xffe7),
        "Space" => Some(0x20),
        _ => None,
    };
    if let Some(sym) = special {
        return sym;
    }

    // 2) 单字符可打印 ASCII：直接用码点（X11 keysym 对 ASCII 与码点一致）
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let cp = c as u32;
        if (32..=126).contains(&cp) {
            return cp;
        }
    }

    // 3) 无法映射（中文、emoji、IME 组合中的按键）→ 不发送
    KEYSYM_NONE
}

/// 输入消息。字段与 `pairdesk_core::protocol::InputMsg` 对齐，
/// 但用更明确的类型，避免 f32/f64 混淆。
#[derive(Debug, Clone, PartialEq)]
pub enum InputMsg {
    MouseMove { x: f64, y: f64 },
    Button { btn: u8, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { keycode: u32, down: bool, mods: u32 },
}

impl From<InputMsg> for CoreInputMsg {
    fn from(m: InputMsg) -> Self {
        match m {
            InputMsg::MouseMove { x, y } => CoreInputMsg::MouseMove { x, y },
            InputMsg::Button { btn, down } => CoreInputMsg::Button { btn, down },
            InputMsg::Scroll { dx, dy } => CoreInputMsg::Scroll { dx, dy },
            InputMsg::Key {
                keycode,
                down,
                mods,
            } => CoreInputMsg::Key {
                keycode,
                down,
                mods,
            },
        }
    }
}

impl InputMsg {
    /// 构造鼠标移动消息（含坐标换算）。
    pub fn mouse_move(rect: &MouseRect, client_x: f32, client_y: f32, remote: (u32, u32)) -> Self {
        let (x, y) = rect.to_remote(client_x, client_y, remote.0 as f32, remote.1 as f32);
        InputMsg::MouseMove { x, y }
    }

    /// 构造按键消息。`sym` 为 0 时调用方应跳过发送。
    pub fn key(sym: u32, down: bool, mods: Modifiers) -> Option<Self> {
        if sym == KEYSYM_NONE {
            return None;
        }
        Some(InputMsg::Key {
            keycode: sym,
            down,
            mods: mods.bits(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 坐标换算 ----------

    #[test]
    fn 坐标换算在无缩放时是恒等映射() {
        // 画面 1:1 显示在 (0,0)，远端也是同样尺寸
        let r = MouseRect::new(0.0, 0.0, 1000.0, 500.0).unwrap();
        let (x, y) = r.to_remote(250.0, 100.0, 1000.0, 500.0);
        assert!((x - 250.0).abs() < 1e-6);
        assert!((y - 100.0).abs() < 1e-6);
    }

    #[test]
    fn 坐标换算按比例放大到远端分辨率() {
        // 画面被缩到 800x450 显示，远端实际是 1920x1080
        let r = MouseRect::new(0.0, 0.0, 800.0, 450.0).unwrap();
        // 正中心
        let (x, y) = r.to_remote(400.0, 225.0, 1920.0, 1080.0);
        assert!((x - 960.0).abs() < 1e-3, "中心 x 应为 960，实际 {x}");
        assert!((y - 540.0).abs() < 1e-3, "中心 y 应为 540，实际 {y}");

        // 右下角
        let (x, y) = r.to_remote(800.0, 450.0, 1920.0, 1080.0);
        assert!((x - 1920.0).abs() < 1e-3);
        assert!((y - 1080.0).abs() < 1e-3);

        // 左上角
        let (x, y) = r.to_remote(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn 坐标换算考虑画面在窗口内的偏移() {
        // 画面居中显示：窗口 1000 宽，画面 800 宽，左边距 100
        let r = MouseRect::new(100.0, 50.0, 800.0, 450.0).unwrap();
        // 点在画面正中心（窗口坐标 500, 275）
        let (x, y) = r.to_remote(500.0, 275.0, 1920.0, 1080.0);
        assert!((x - 960.0).abs() < 1e-3, "实际 {x}");
        assert!((y - 540.0).abs() < 1e-3, "实际 {y}");
    }

    #[test]
    fn 坐标换算把越界坐标夹紧到画面边界() {
        // 这是关键行为：鼠标拖到画面外不能产生越界坐标
        let r = MouseRect::new(100.0, 50.0, 800.0, 450.0).unwrap();

        // 左上越界
        let (x, y) = r.to_remote(-500.0, -500.0, 1920.0, 1080.0);
        assert_eq!((x, y), (0.0, 0.0));

        // 右下越界 → 夹到远端右下角
        let (x, y) = r.to_remote(9999.0, 9999.0, 1920.0, 1080.0);
        assert!((x - 1920.0).abs() < 1e-3, "实际 {x}");
        assert!((y - 1080.0).abs() < 1e-3, "实际 {y}");
    }

    #[test]
    fn 非法矩形拒绝换算() {
        // 零尺寸 / 负尺寸 / 非有限值都不应产生矩形（旧实现返回 null 跳过发送）
        assert!(MouseRect::new(0.0, 0.0, 0.0, 100.0).is_none());
        assert!(MouseRect::new(0.0, 0.0, 100.0, 0.0).is_none());
        assert!(MouseRect::new(0.0, 0.0, -10.0, 100.0).is_none());
        assert!(MouseRect::new(0.0, 0.0, f32::NAN, 100.0).is_none());
        assert!(MouseRect::new(0.0, 0.0, 100.0, f32::INFINITY).is_none());
    }

    // ---------- 鼠标按键 ----------

    #[test]
    fn 鼠标按键映射与旧实现一致() {
        // 浏览器：0=左, 1=中, 2=右 → 协议：1=左, 2=中, 3=右
        assert_eq!(map_mouse_button(0), BTN_LEFT);
        assert_eq!(map_mouse_button(1), BTN_MIDDLE);
        assert_eq!(map_mouse_button(2), BTN_RIGHT);
    }

    #[test]
    fn 未知鼠标按键回落到左键() {
        // 旧实现的三元表达式末尾 `: 1`，即未知值一律当左键
        assert_eq!(map_mouse_button(3), BTN_LEFT);
        assert_eq!(map_mouse_button(4), BTN_LEFT);
        assert_eq!(map_mouse_button(255), BTN_LEFT);
    }

    // ---------- 修饰键 ----------

    #[test]
    fn 修饰键位掩码与内核常量一致() {
        assert_eq!(Modifiers::default().bits(), 0);
        assert_eq!(
            Modifiers {
                shift: true,
                ..Default::default()
            }
            .bits(),
            1
        );
        assert_eq!(
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
            .bits(),
            2
        );
        assert_eq!(
            Modifiers {
                alt: true,
                ..Default::default()
            }
            .bits(),
            4
        );
        assert_eq!(
            Modifiers {
                meta: true,
                ..Default::default()
            }
            .bits(),
            8
        );
    }

    #[test]
    fn 多修饰键可叠加() {
        let all = Modifiers {
            shift: true,
            ctrl: true,
            alt: true,
            meta: true,
        };
        assert_eq!(all.bits(), 15);

        let ctrl_shift = Modifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(ctrl_shift.bits(), 3);
    }

    // ---------- 按键映射 ----------

    #[test]
    fn 特殊键映射为正确的_x11_keysym() {
        assert_eq!(key_to_sym("Backspace", "Backspace"), 0xff08);
        assert_eq!(key_to_sym("Tab", "Tab"), 0xff09);
        assert_eq!(key_to_sym("Enter", "Enter"), 0xff0d);
        assert_eq!(key_to_sym("NumpadEnter", "Enter"), 0xff0d);
        assert_eq!(key_to_sym("Escape", "Escape"), 0xff1b);
        assert_eq!(key_to_sym("Delete", "Delete"), 0xffff);
        assert_eq!(key_to_sym("Space", " "), 0x20);
    }

    #[test]
    fn 方向键与导航键映射正确() {
        assert_eq!(key_to_sym("ArrowLeft", "ArrowLeft"), 0xff51);
        assert_eq!(key_to_sym("ArrowUp", "ArrowUp"), 0xff52);
        assert_eq!(key_to_sym("ArrowRight", "ArrowRight"), 0xff53);
        assert_eq!(key_to_sym("ArrowDown", "ArrowDown"), 0xff54);
        assert_eq!(key_to_sym("Home", "Home"), 0xff50);
        assert_eq!(key_to_sym("End", "End"), 0xff57);
        assert_eq!(key_to_sym("PageUp", "PageUp"), 0xff55);
        assert_eq!(key_to_sym("PageDown", "PageDown"), 0xff56);
    }

    #[test]
    fn 修饰键本身映射为_x11_keysym() {
        assert_eq!(key_to_sym("ShiftLeft", "Shift"), 0xffe1);
        assert_eq!(key_to_sym("ShiftRight", "Shift"), 0xffe1);
        assert_eq!(key_to_sym("ControlLeft", "Control"), 0xffe3);
        assert_eq!(key_to_sym("ControlRight", "Control"), 0xffe3);
        assert_eq!(key_to_sym("AltLeft", "Alt"), 0xffe9);
        assert_eq!(key_to_sym("AltRight", "Alt"), 0xffe9);
        assert_eq!(key_to_sym("MetaLeft", "Meta"), 0xffe7);
        assert_eq!(key_to_sym("MetaRight", "Meta"), 0xffe7);
    }

    #[test]
    fn 可打印_ascii_用码点作为_keysym() {
        // X11 keysym 对 ASCII 可打印区与 Unicode 码点一致
        assert_eq!(key_to_sym("KeyA", "a"), 0x61);
        assert_eq!(key_to_sym("KeyA", "A"), 0x41); // Shift+a
        assert_eq!(key_to_sym("Digit1", "1"), 0x31);
        assert_eq!(key_to_sym("Semicolon", ";"), 0x3b);
        assert_eq!(key_to_sym("Minus", "-"), 0x2d);
        assert_eq!(key_to_sym("KeyZ", "z"), 0x7a);
    }

    #[test]
    fn 空格用特殊键路径优先于字符路径() {
        // "Space" 的 key 是 " "（0x20），两条路径结果相同，但要确保不冲突
        assert_eq!(key_to_sym("Space", " "), 0x20);
        assert_eq!(key_to_sym("Space", " "), 0x20);
    }

    #[test]
    fn 非_ascii_与_ime_按键返回不发送() {
        // 中文、emoji、IME 组合中的按键必须返回 0（不发送），
        // 否则会在远端产生脏键码
        assert_eq!(key_to_sym("KeyA", "中"), KEYSYM_NONE);
        assert_eq!(key_to_sym("KeyA", "😀"), KEYSYM_NONE);
        assert_eq!(key_to_sym("", ""), KEYSYM_NONE);
        assert_eq!(key_to_sym("Unidentified", "Unidentified"), KEYSYM_NONE);
        // 多字符 key（如 "Dead"）也不发送
        assert_eq!(key_to_sym("Unknown", "ab"), KEYSYM_NONE);
    }

    #[test]
    fn 控制字符不发送() {
        // \n \t \0 等控制字符不是可打印 ASCII，应走"不发送"
        assert_eq!(key_to_sym("Unknown", "\n"), KEYSYM_NONE);
        assert_eq!(key_to_sym("Unknown", "\t"), KEYSYM_NONE);
        assert_eq!(key_to_sym("Unknown", "\0"), KEYSYM_NONE);
    }

    // ---------- 消息构造 ----------

    #[test]
    fn 按键消息在无法映射时不构造() {
        assert_eq!(InputMsg::key(KEYSYM_NONE, true, Modifiers::default()), None);

        let m = InputMsg::key(0x61, true, Modifiers::default()).unwrap();
        assert_eq!(
            m,
            InputMsg::Key {
                keycode: 0x61,
                down: true,
                mods: 0
            }
        );
    }

    #[test]
    fn 鼠标移动消息自动完成坐标换算() {
        let r = MouseRect::new(0.0, 0.0, 800.0, 450.0).unwrap();
        let m = InputMsg::mouse_move(&r, 400.0, 225.0, (1920, 1080));
        match m {
            InputMsg::MouseMove { x, y } => {
                assert!((x - 960.0).abs() < 1e-3);
                assert!((y - 540.0).abs() < 1e-3);
            }
            _ => panic!("应为 MouseMove"),
        }
    }

    #[test]
    fn 输入消息可转换为内核协议类型() {
        let core: CoreInputMsg = InputMsg::Button {
            btn: BTN_LEFT,
            down: true,
        }
        .into();
        match core {
            CoreInputMsg::Button { btn, down } => {
                assert_eq!(btn, BTN_LEFT);
                assert!(down);
            }
            _ => panic!("应为 Button"),
        }
    }
}
