//! Windows 输入注入 —— SendInput（M4 里程碑真实实现）。
//!
//! 用 SendInput 把鼠标/键盘事件塞进全局输入流。
//! 组合键（mods）实现为"先按修饰键→按主键→松开"的序列。
//!
//! ⚠️ 真机约束：注入需在交互式桌面会话运行（普通用户即可，无需管理员；
//! 安全桌面/UAC 提升窗口上注入无效属系统行为）。

use anyhow::{anyhow, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT,
    VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, WHEEL_DELTA,
};

use super::{keysym, InputInjector};

pub struct WinInjector;

impl WinInjector {
    pub fn new() -> Result<WinInjector> {
        Ok(WinInjector)
    }
}

impl InputInjector for WinInjector {
    fn move_mouse(&mut self, x: f64, y: f64) -> Result<()> {
        // 绝对坐标：0..65535 映射全屏
        let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f64;
        let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f64;
        let dx = ((x / sw) * 65535.0).clamp(0.0, 65535.0) as u32;
        let dy = ((y / sh) * 65535.0).clamp(0.0, 65535.0) as u32;
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: dx as i32,
                    dy: dy as i32,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[input]);
        Ok(())
    }

    fn button(&mut self, btn: u8, down: bool) -> Result<()> {
        use super::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT};
        let flags = match (btn, down) {
            (BTN_LEFT, true) => MOUSEEVENTF_LEFTDOWN,
            (BTN_LEFT, false) => MOUSEEVENTF_LEFTUP,
            (BTN_RIGHT, true) => MOUSEEVENTF_RIGHTDOWN,
            (BTN_RIGHT, false) => MOUSEEVENTF_RIGHTUP,
            (BTN_MIDDLE, true) => MOUSEEVENTF_MIDDLEDOWN,
            (BTN_MIDDLE, false) => MOUSEEVENTF_MIDDLEUP,
            _ => return Err(anyhow!("不支持的鼠标键 {btn}")),
        };
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[input]);
        Ok(())
    }

    fn scroll(&mut self, _dx: f64, dy: f64) -> Result<()> {
        let data = (dy * WHEEL_DELTA as f64).round() as i32; // 正=上滚
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: data as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[input]);
        Ok(())
    }

    fn key(&mut self, sym: u32, down: bool, mods: u32) -> Result<()> {
        let vk = vk_for_sym(sym).ok_or_else(|| anyhow!("未映射 keysym 0x{sym:x}"))?;
        let mod_vks = mods_to_vks(mods);
        if down {
            // 先按修饰键，再按主键
            for mvk in mod_vks.iter() {
                send_key(*mvk, true);
            }
            send_key(vk, true);
        } else {
            // 先松主键，再松修饰键
            send_key(vk, false);
            for mvk in mod_vks.iter().rev() {
                send_key(*mvk, false);
            }
        }
        Ok(())
    }
}

/// 发送一条 SendInput（鼠标或键盘）。
fn send(items: &[INPUT]) {
    unsafe {
        SendInput(items, std::mem::size_of::<INPUT>() as i32);
    }
}

fn send_key(vk: u16, down: bool) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send(&[input]);
}

/// 修饰键位 → VK 序列（按固定顺序，保证组合键稳定）。
fn mods_to_vks(mods: u32) -> Vec<u16> {
    use super::{MOD_ALT, MOD_CTRL, MOD_META, MOD_SHIFT};
    let mut v = Vec::new();
    if mods & MOD_CTRL != 0 {
        v.push(VK_CONTROL.0 as u16);
    }
    if mods & MOD_ALT != 0 {
        v.push(VK_MENU.0 as u16);
    }
    if mods & MOD_SHIFT != 0 {
        v.push(VK_SHIFT.0 as u16);
    }
    if mods & MOD_META != 0 {
        v.push(VK_LWIN.0 as u16);
    }
    v
}

/// X11 keysym → Windows 虚拟键码（VK）。返回 None 表示无映射。
fn vk_for_sym(sym: u32) -> Option<u16> {
    // 1) ASCII 字母/数字：VK 就是大写 ASCII 码
    if (0x30..=0x39).contains(&sym) || (0x41..=0x5a).contains(&sym) {
        return Some(sym as u16);
    }
    if (0x61..=0x7a).contains(&sym) {
        return Some((sym - 0x20) as u16); // 小写→大写
    }
    // 2) 功能/导航键（keysym 0xff00+）
    Some(match sym {
        keysym::RETURN => VK_RETURN.0 as u16,
        keysym::TAB => VK_TAB.0 as u16,
        keysym::SPACE => VK_SPACE.0 as u16,
        keysym::BACKSPACE => VK_BACK.0 as u16,
        keysym::ESCAPE => VK_ESCAPE.0 as u16,
        keysym::DELETE => VK_DELETE.0 as u16,
        keysym::LEFT => VK_LEFT.0 as u16,
        keysym::RIGHT => VK_RIGHT.0 as u16,
        keysym::UP => VK_UP.0 as u16,
        keysym::DOWN => VK_DOWN.0 as u16,
        keysym::HOME => VK_HOME.0 as u16,
        keysym::END => VK_END.0 as u16,
        keysym::PAGE_UP => VK_PRIOR.0 as u16,
        keysym::PAGE_DOWN => VK_NEXT.0 as u16,
        keysym::SHIFT_L => VK_SHIFT.0 as u16,
        keysym::CTRL_L => VK_CONTROL.0 as u16,
        keysym::ALT_L => VK_MENU.0 as u16,
        keysym::META_L | keysym::SUPER_L => VK_LWIN.0 as u16,
        _ => return None,
    })
}
