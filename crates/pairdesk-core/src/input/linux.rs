//! Linux XTest 输入注入。
//!
//! 通过 x11rb 的 XTEST 扩展注入：鼠标移动/按键/滚轮/键盘。
//! XTEST 只有一个底层原语 `xtest_fake_input`，用 X 事件类型区分动作：
//! KeyPress(2)/KeyRelease(3)/ButtonPress(4)/ButtonRelease(5)/MotionNotify(6)。
//! 需要 X server 支持 XTEST（Xvfb/Xorg 默认支持）。

use std::collections::HashMap;

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as XProtoConnectionExt;
use x11rb::protocol::xtest::ConnectionExt as XtestConnectionExt;

use super::{
    InputInjector, KeySym, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_WHEEL_DOWN, BTN_WHEEL_UP, MOD_ALT,
    MOD_CTRL, MOD_META, MOD_SHIFT,
};

// X 事件类型
const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;
const BUTTON_PRESS: u8 = 4;
const BUTTON_RELEASE: u8 = 5;
const MOTION_NOTIFY: u8 = 6;
/// XTEST 的 deviceid: 0 = 核心设备（默认指针/键盘）
const DEVICE_CORE: u8 = 0;

pub struct X11Injector {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
    root: x11rb::protocol::xproto::Window,
    /// 键盘映射缓存：keysym → 最小 keycode
    keymap: HashMap<u32, u8>,
}

impl X11Injector {
    pub fn new() -> Result<X11Injector> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen_num].root;
        let keymap = build_keymap(&conn, screen_num)?;
        Ok(X11Injector {
            conn,
            screen_num,
            root,
            keymap,
        })
    }

    /// 底层原语：向 X server 注入一个假事件。
    fn fake_input(&self, event_type: u8, detail: u8, x: i16, y: i16) -> Result<()> {
        self.conn
            .xtest_fake_input(event_type, detail, 0, self.root, x, y, DEVICE_CORE)?;
        Ok(())
    }
}

impl InputInjector for X11Injector {
    fn move_mouse(&mut self, x: f64, y: f64) -> Result<()> {
        self.fake_input(MOTION_NOTIFY, 0, x.round() as i16, y.round() as i16)
    }

    fn button(&mut self, btn: u8, down: bool) -> Result<()> {
        let b = match btn {
            BTN_LEFT => 1,
            BTN_MIDDLE => 2,
            BTN_RIGHT => 3,
            BTN_WHEEL_UP => 4,
            BTN_WHEEL_DOWN => 5,
            _ => return Ok(()), // 未知按键忽略，避免抖动导致断线
        };
        self.fake_input(if down { BUTTON_PRESS } else { BUTTON_RELEASE }, b, 0, 0)
    }

    fn scroll(&mut self, _dx: f64, dy: f64) -> Result<()> {
        let steps = dy.abs().max(1.0) as i32;
        let dir = if dy >= 0.0 { 4 } else { 5 }; // up/down
        for _ in 0..steps.max(1) {
            self.fake_input(BUTTON_PRESS, dir, 0, 0)?;
            self.fake_input(BUTTON_RELEASE, dir, 0, 0)?;
        }
        Ok(())
    }

    fn key(&mut self, sym: KeySym, down: bool, mods: u32) -> Result<()> {
        let ev = if down { KEY_PRESS } else { KEY_RELEASE };
        // 修饰键状态（协议层已拆成独立事件，保持组合键语义）
        if mods & MOD_SHIFT != 0 {
            self.fake_input(ev, self.keycode_of(0xffe1)?, 0, 0)?;
        }
        if mods & MOD_CTRL != 0 {
            self.fake_input(ev, self.keycode_of(0xffe3)?, 0, 0)?;
        }
        if mods & MOD_ALT != 0 {
            self.fake_input(ev, self.keycode_of(0xffe9)?, 0, 0)?;
        }
        if mods & MOD_META != 0 {
            self.fake_input(ev, self.keycode_of(0xffe7)?, 0, 0)?;
        }
        // 主键
        let kc = self.keycode_of(sym)?;
        self.fake_input(ev, kc, 0, 0)
    }
}

impl X11Injector {
    fn keycode_of(&self, sym: KeySym) -> Result<u8> {
        self.keymap
            .get(&sym)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("keysym 0x{:x} 无对应 keycode", sym))
    }
}

/// 构建键盘映射表：遍历 X 键盘映射，记录每个 keysym 的最小 keycode。
fn build_keymap<C: Connection>(
    conn: &C,
    screen_num: usize,
) -> Result<HashMap<u32, u8>> {
    let setup = &conn.setup();
    let first = setup.min_keycode;
    let per = setup.max_keycode - first + 1;
    let reply = conn
        .get_keyboard_mapping(first, per)?
        .reply()?;
    let mut map: HashMap<u32, u8> = HashMap::new();
    for (i, syms) in reply.keysyms.chunks(reply.keysyms_per_keycode as usize).enumerate() {
        let keycode = first + i as u8;
        for sym in syms {
            if *sym != 0 {
                // 保留最小 keycode（同 keysym 可能映射到多个 keycode）
                map.entry(*sym).or_insert(keycode);
            }
        }
    }
    let _ = screen_num;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_has_basic_keys() {
        // 需要真实 X server 才能连接；无 DISPLAY 时跳过
        let Ok(inj) = X11Injector::new() else {
            return;
        };
        assert!(inj.keymap.contains_key(&('a' as u32)));
        assert!(inj.keymap.contains_key(&0xffe1)); // Shift_L
    }
}