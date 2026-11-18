//! PairDesk 应用层逻辑 —— 与 UI 框架无关。
//!
//! # 为什么要这个 crate
//!
//! 迁移到 GPUI 前，PairDesk 的业务逻辑分散在两处、都不可单测：
//!
//! - `crates/pairdesk-app/src/bridge.rs`：逻辑写在 `#[tauri::command]` 函数里，
//!   必须起 Tauri 运行时才能调用。
//! - `app/src/**/*.tsx`：会话状态机、输入坐标映射、按键映射写在 React 组件里，
//!   必须起浏览器才能验证。
//!
//! 本 crate 把这些行为原样抽出成纯函数 + 纯状态机，使它们可以在 `cargo test`
//! 里被穷举验证。抽出后：
//!
//! - Tauri 壳与 GPUI 壳都只负责"把状态渲染成像素"和"把用户操作转成命令"。
//! - 换 UI 框架不再需要重新推导这些行为——本 crate 的单测即规格。
//!
//! # 模块
//!
//! - [`credentials`]：会话码 / 密码生成（原 `HostPage.tsx` 的 `genSid`/`genPwd`）
//! - [`input`]：鼠标坐标换算、按键 keysym 映射、修饰键位（原 `VideoView.tsx`）
//! - [`session`]：会话状态机（原 `useSession.ts` + `bridge.rs` 的阶段推导）
//! - [`update`]：版本比较与平台资产匹配（原 `bridge.rs::pd_check_update`）
//! - [`permissions`]：权限引导判定 + 平台权限操作（原 `PermissionBanner.tsx`）

pub mod credentials;
pub mod input;
pub mod permissions;
pub mod session;
pub mod update;

pub use credentials::{gen_password, gen_sid, CredentialPair};
pub use input::{InputMsg, MouseRect};
pub use session::{SessionPhase, SessionState, StatusTone};
pub use update::{platform_asset_pattern, UpdateInfo};
