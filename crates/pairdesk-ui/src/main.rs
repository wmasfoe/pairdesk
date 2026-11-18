//! PairDesk GPUI 桌面壳入口。
//!
//! 分层：本 crate 是**适配器层**，只做渲染与事件转发。
//! 业务逻辑在 `pairdesk-shell`（有单测），引擎在 `pairdesk-core`。

mod app_view;
mod frame;
mod permission_exec;
mod video_view;
mod session_host;
mod theme;
mod update;

use gpui::{px, App, Application, Bounds, WindowBounds, WindowOptions};
use gpui::prelude::*;
use gpui_component::Root;

use app_view::PairDeskApp;

fn main() {
    Application::new().run(|cx: &mut App| {
        // 初始化 gpui-component（主题、全局状态、各类组件）
        gpui_component::init(cx);

        // 系统语言是中文时用中文界面
        gpui_component::set_locale("zh-CN");

        let bounds = Bounds::centered(
            None,
            gpui::size(
                px(theme::size::window_width()),
                px(theme::size::window_height()),
            ),
            cx,
        );

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("PairDesk 远程协助".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                // 需要外层 Root 包裹，否则 Modal/Popover 等浮层无法渲染
                let view = cx.new(|cx| PairDeskApp::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("打开主窗口失败");

        cx.activate(true);
    });
}
