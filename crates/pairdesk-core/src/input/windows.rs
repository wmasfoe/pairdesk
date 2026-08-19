//! Windows 输入注入（SendInput）—— M4 里程碑实现。
//! 当前为占位，仅在 Windows 编译时启用。

use anyhow::{bail, Result};

use super::InputInjector;

pub struct WinInjector;

impl WinInjector {
    /// 占位构造：仅保证 Windows 分支可编译；真实注入实现在 M4 里程碑
    pub fn new() -> Result<WinInjector> {
        Ok(WinInjector)
    }
}

impl InputInjector for WinInjector {
    fn move_mouse(&mut self, _x: f64, _y: f64) -> anyhow::Result<()> {
        bail!("Windows 输入注入尚未实现（M4 里程碑）");
    }
    fn button(&mut self, _btn: u8, _down: bool) -> anyhow::Result<()> {
        bail!("Windows 输入注入尚未实现（M4 里程碑）");
    }
    fn scroll(&mut self, _dx: f64, _dy: f64) -> anyhow::Result<()> {
        bail!("Windows 输入注入尚未实现（M4 里程碑）");
    }
    fn key(&mut self, _sym: u32, _down: bool, _mods: u32) -> anyhow::Result<()> {
        bail!("Windows 输入注入尚未实现（M4 里程碑）");
    }
}