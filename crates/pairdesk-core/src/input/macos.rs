//! macOS 输入注入 —— CGEvent（M2 里程碑真实实现）。
//!
//! 用 CoreGraphics 的 CGEvent 把一个事件塞进全局事件流(HID tap)，
//! 从而驱动鼠标移动/按键/滚轮/键盘。
//!
//! ⚠️ 真机约束：注入需「辅助功能」权限（系统设置→隐私与安全→辅助功能，
//! 授予运行 PairDesk 的终端/应用），否则 post 会被系统忽略。

use anyhow::{bail, Result};
use core_graphics::event::{
    CGMouseButton, CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

use super::{keysym, InputInjector};

pub struct MacInjector;

impl MacInjector {
    pub fn new() -> Result<MacInjector> {
        Ok(MacInjector)
    }
}

impl InputInjector for MacInjector {
    fn move_mouse(&mut self, x: f64, y: f64) -> Result<()> {
        let ev = CGEvent::new_mouse_event(
            event_source()?,
            CGEventType::MouseMoved,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| anyhow::anyhow!("创建鼠标移动事件失败"))?;
        ev.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn button(&mut self, btn: u8, down: bool) -> Result<()> {
        use super::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT};
        let (mouse_type, mb) = match (btn, down) {
            (BTN_LEFT, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (BTN_LEFT, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (BTN_RIGHT, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (BTN_RIGHT, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (BTN_MIDDLE, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            (BTN_MIDDLE, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            _ => bail!("不支持的鼠标键 {btn}"),
        };
        // 取当前鼠标位置（让键击发生在光标处）
        let pos = mouse_location();
        let ev = CGEvent::new_mouse_event(event_source()?, mouse_type, pos, mb)
            .map_err(|_| anyhow::anyhow!("创建鼠标按键事件失败"))?;
        ev.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn scroll(&mut self, _dx: f64, dy: f64) -> Result<()> {
        let ev = CGEvent::new_scroll_event(
            event_source()?,
            ScrollEventUnit::PIXEL,
            1,
            dy as i32,
            0,
            0,
        )
        .map_err(|_| anyhow::anyhow!("创建滚动事件失败"))?;
        ev.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn key(&mut self, sym: u32, down: bool, mods: u32) -> Result<()> {
        let Some(keycode) = keycode_for_sym(sym) else {
            // 未映射的按键静默忽略（避免注入失败干扰）。
            return Ok(());
        };
        let ev = CGEvent::new_keyboard_event(event_source()?, keycode, down)
            .map_err(|_| anyhow::anyhow!("创建键盘事件失败"))?;
        ev.set_flags(flags_for_mods(mods));
        ev.post(CGEventTapLocation::HID);
        Ok(())
    }
}

/// 新建一个全局 HID 系统状态的 CGEventSource（事件注入用）。
fn event_source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("创建 CGEventSource 失败"))
}

/// 当前鼠标所在位置（用于把"按下/松开"落到光标处）。
fn mouse_location() -> CGPoint {
    match CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
        Ok(source) => match CGEvent::new(source) {
            Ok(ev) => ev.location(),
            Err(_) => CGPoint::new(0.0, 0.0),
        },
        Err(_) => CGPoint::new(0.0, 0.0),
    }
}

/// 修饰键位 → CGEventFlags。
fn flags_for_mods(mods: u32) -> CGEventFlags {
    use super::{MOD_ALT, MOD_CTRL, MOD_META, MOD_SHIFT};
    let mut f = CGEventFlags::empty();
    if mods & MOD_SHIFT != 0 {
        f.insert(CGEventFlags::CGEventFlagShift);
    }
    if mods & MOD_CTRL != 0 {
        f.insert(CGEventFlags::CGEventFlagControl);
    }
    if mods & MOD_ALT != 0 {
        f.insert(CGEventFlags::CGEventFlagAlternate);
    }
    if mods & MOD_META != 0 {
        f.insert(CGEventFlags::CGEventFlagCommand);
    }
    f
}

/// X11 keysym → macOS 虚拟键码。返回 None 表示无映射。
fn keycode_for_sym(sym: u32) -> Option<u16> {
    // 1) ASCII 字母 a-z
    if (0x61..=0x7a).contains(&sym) {
        return Some(LETTER_KEYS[(sym - 0x61) as usize]);
    }
    // 2) ASCII 数字 0-9
    if (0x30..=0x39).contains(&sym) {
        return Some(DIGIT_KEYS[(sym - 0x30) as usize]);
    }
    // 3) 常见 ASCII 符号
    if let Some(k) = ascii_symbol_keycode(sym as u8) {
        return Some(k);
    }
    // 4) 功能/导航键（keysym 0xff00+）
    Some(match sym {
        keysym::RETURN => 36,
        keysym::TAB => 48,
        keysym::SPACE => 49,
        keysym::BACKSPACE => 51,
        keysym::ESCAPE => 53,
        keysym::DELETE => 117,
        keysym::LEFT => 123,
        keysym::RIGHT => 124,
        keysym::DOWN => 125,
        keysym::UP => 126,
        keysym::HOME => 115,
        keysym::END => 119,
        keysym::PAGE_UP => 116,
        keysym::PAGE_DOWN => 121,
        keysym::SHIFT_L | keysym::META_L => 56,
        keysym::CTRL_L => 59,
        keysym::ALT_L => 58,
        keysym::SUPER_L => 55,
        _ => return None,
    })
}

fn ascii_symbol_keycode(c: u8) -> Option<u16> {
    Some(match c {
        b'-' => 27,
        b'=' => 24,
        b'[' => 33,
        b']' => 30,
        b'\\' => 42,
        b';' => 41,
        b'\'' => 39,
        b',' => 43,
        b'.' => 47,
        b'/' => 44,
        b'`' => 50,
        b'\n' => 36,
        _ => return None,
    })
}

const LETTER_KEYS: [u16; 26] = [
    0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6,
];
const DIGIT_KEYS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
