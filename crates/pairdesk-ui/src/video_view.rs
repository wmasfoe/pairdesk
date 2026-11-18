//! 远程画面视图：渲染 + 鼠标键盘捕获。
//!
//! # 为什么单独成模块
//!
//! 画面区是整个 App 唯一的交互热点：鼠标移动、点击、滚轮、键盘都要在这里
//! 捕获并转发到远端。逻辑虽多，但**所有换算都在 `pairdesk_shell::input`**，
//! 本模块只负责「拿到窗口坐标 → 交给 shell → 发出消息」。
//!
//! # 坐标难点
//!
//! GPUI 的事件给的是**窗口坐标**，而远端需要的是**画面矩形内的相对坐标**。
//! 因此必须知道画面元素的 bounds。做法：用 `canvas` 在 prepaint 阶段把 bounds
//! 写进共享状态，事件处理器再读出来。

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    canvas, div, img, prelude::*, px, Bounds, FocusHandle,
    IntoElement, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, RenderImage, ScrollDelta, ScrollWheelEvent, Window,
};
use pairdesk_shell::input::{self, InputMsg, Modifiers, MouseRect};

/// 画面矩形的共享状态（canvas 写入，事件处理器读取）。
pub type SharedBounds = Rc<Cell<Option<MouseRect>>>;

/// 创建共享 bounds 槽。
pub fn new_shared_bounds() -> SharedBounds {
    Rc::new(Cell::new(None))
}

/// 构造画面区域元素。
///
/// `on_input` 在每次捕获到输入时被调用，参数是已经换算好的 [`InputMsg`]。
#[allow(clippy::too_many_arguments)]
pub fn video_view(
    image: Option<Arc<RenderImage>>,
    remote_size: (u32, u32),
    bounds_slot: SharedBounds,
    focus: FocusHandle,
    bg: gpui::Rgba,
    muted: gpui::Rgba,
    waiting_text: &'static str,
    on_input: Rc<dyn Fn(InputMsg, &mut Window, &mut gpui::App)>,
) -> impl IntoElement {
    // 记录画面矩形：canvas 的 prepaint 能拿到元素在窗口内的实际位置与尺寸
    let bounds_for_paint = bounds_slot.clone();
    let record_bounds = canvas(
        move |bounds: Bounds<Pixels>, _window, _cx| {
            let rect = MouseRect::new(
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
            bounds_for_paint.set(rect);
        },
        |_bounds, _state, _window, _cx| {},
    )
    .absolute()
    .size_full();

    // 四个输入处理器的克隆
    // 每个事件一个独立副本：闭包是 move 的，共用会冲突
    let move_handler = on_input.clone();
    let down_left = on_input.clone();
    let down_right = on_input.clone();
    let down_middle = on_input.clone();
    let up_left = on_input.clone();
    let up_right = on_input.clone();
    let up_middle = on_input.clone();
    let scroll_handler = on_input.clone();
    let key_down_handler = on_input.clone();
    let key_up_handler = on_input;

    let slot_move = bounds_slot;

    let content = match image {
        Some(img_data) => img(img_data)
            .size_full()
            .object_fit(gpui::ObjectFit::Contain)
            .into_any_element(),
        None => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.))
            .text_color(muted)
            .child(waiting_text)
            .into_any_element(),
    };

    div()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .relative()
        .overflow_hidden()
        .rounded(px(14.))
        .bg(bg)
        .track_focus(&focus)
        .cursor_crosshair()
        .child(record_bounds)
        .child(content)
        // 鼠标移动：换算为远端绝对坐标
        .on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
            if let Some(msg) = mouse_move_msg(&slot_move, e, remote_size) {
                move_handler(msg, window, cx);
            }
        })
        // 鼠标按下
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
            if let Some(msg) = button_msg(e, true) {
                down_left(msg, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
            if let Some(msg) = button_msg(e, true) {
                down_right(msg, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Middle, move |e: &MouseDownEvent, window, cx| {
            if let Some(msg) = button_msg(e, true) {
                down_middle(msg, window, cx);
            }
        })
        // 鼠标松开
        .on_mouse_up(MouseButton::Left, move |e: &MouseUpEvent, window, cx| {
            if let Some(msg) = button_up_msg(e, false) {
                up_left(msg, window, cx);
            }
        })
        .on_mouse_up(MouseButton::Right, move |e: &MouseUpEvent, window, cx| {
            if let Some(msg) = button_up_msg(e, false) {
                up_right(msg, window, cx);
            }
        })
        .on_mouse_up(MouseButton::Middle, move |e: &MouseUpEvent, window, cx| {
            if let Some(msg) = button_up_msg(e, false) {
                up_middle(msg, window, cx);
            }
        })
        // 滚轮
        .on_scroll_wheel(move |e: &ScrollWheelEvent, window, cx| {
            let (dx, dy) = scroll_delta(e);
            scroll_handler(InputMsg::Scroll { dx, dy }, window, cx);
        })
        // 键盘
        .on_key_down(move |e: &KeyDownEvent, window, cx| {
            if let Some(msg) = key_msg(e, true) {
                key_down_handler(msg, window, cx);
            }
        })
        .on_key_up(move |e: &KeyUpEvent, window, cx| {
            if let Some(msg) = key_msg_up(e) {
                key_up_handler(msg, window, cx);
            }
        })
}

/// 把窗口坐标换算为远端坐标并构造鼠标移动消息。
fn mouse_move_msg(
    slot: &SharedBounds,
    e: &MouseMoveEvent,
    remote_size: (u32, u32),
) -> Option<InputMsg> {
    let rect = slot.get()?;
    // 远端分辨率来自内核的 Size 事件；未收到前用占位值，收到后自动生效
    let (rw, rh) = (remote_size.0 as f32, remote_size.1 as f32);
    let (x, y) = rect.to_remote(
        f32::from(e.position.x),
        f32::from(e.position.y),
        rw,
        rh,
    );
    Some(InputMsg::MouseMove { x, y })
}

/// 构造鼠标按下消息。
fn button_msg(e: &MouseDownEvent, down: bool) -> Option<InputMsg> {
    Some(InputMsg::Button {
        btn: map_gpui_button(e.button),
        down,
    })
}

/// 构造鼠标松开消息。
fn button_up_msg(e: &MouseUpEvent, down: bool) -> Option<InputMsg> {
    Some(InputMsg::Button {
        btn: map_gpui_button(e.button),
        down,
    })
}

/// GPUI 鼠标按键 → 协议按键号。
///
/// 注意：这里不复用 `shell::input::map_mouse_button`，因为那个函数接收的是
/// **浏览器**的按键编号（0=左,1=中,2=右），而 GPUI 用的是枚举。两者语义不同，
/// 混用会点错键。协议侧编号仍复用 shell 的常量。
fn map_gpui_button(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => input::BTN_LEFT,
        MouseButton::Right => input::BTN_RIGHT,
        MouseButton::Middle => input::BTN_MIDDLE,
        // 侧键等暂不支持，当左键处理
        _ => input::BTN_LEFT,
    }
}

/// 滚轮增量归一化。
///
/// GPUI 有两种 delta：精确像素（触控板）与行数（传统滚轮）。
/// 行数按每行 40px 折算，与浏览器 `deltaY` 量级接近。
fn scroll_delta(e: &ScrollWheelEvent) -> (f64, f64) {
    match e.delta {
        ScrollDelta::Pixels(p) => (f64::from(p.x), f64::from(p.y)),
        ScrollDelta::Lines(p) => (f64::from(p.x) * 40.0, f64::from(p.y) * 40.0),
    }
}

/// GPUI 修饰键 → shell 的修饰键位。
fn to_modifiers(m: gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: m.shift,
        ctrl: m.control,
        alt: m.alt,
        // platform 在 macOS 是 Cmd、Windows 是 Win、Linux 是 Super —— 都映射到 meta
        meta: m.platform,
    }
}

/// 构造按键按下消息。
fn key_msg(e: &KeyDownEvent, down: bool) -> Option<InputMsg> {
    let ks = &e.keystroke;
    let sym = input::key_to_sym(&ks.key, ks.key_char.as_deref().unwrap_or(&ks.key));
    InputMsg::key(sym, down, to_modifiers(ks.modifiers))
}

/// 构造按键松开消息。
fn key_msg_up(e: &KeyUpEvent) -> Option<InputMsg> {
    let ks = &e.keystroke;
    let sym = input::key_to_sym(&ks.key, ks.key_char.as_deref().unwrap_or(&ks.key));
    InputMsg::key(sym, false, to_modifiers(ks.modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpui_鼠标按键映射到协议编号() {
        // 协议编号来自 pairdesk-core::input：1=左 2=中 3=右
        assert_eq!(map_gpui_button(MouseButton::Left), 1);
        assert_eq!(map_gpui_button(MouseButton::Middle), 2);
        assert_eq!(map_gpui_button(MouseButton::Right), 3);
    }

    #[test]
    fn 修饰键映射覆盖全部四个() {
        let m = gpui::Modifiers {
            control: true,
            alt: true,
            shift: true,
            platform: true,
            function: false,
        };
        let mods = to_modifiers(m);
        assert!(mods.shift);
        assert!(mods.ctrl);
        assert!(mods.alt);
        assert!(mods.meta);
        // 位掩码 1|2|4|8 = 15
        assert_eq!(mods.bits(), 15);
    }

    #[test]
    fn 无修饰键时位掩码为零() {
        assert_eq!(to_modifiers(gpui::Modifiers::default()).bits(), 0);
    }

    #[test]
    fn 行数滚轮按四十像素折算() {
        // 传统滚轮一格 = 1 行 → 40px，与浏览器 deltaY 量级接近
        let e = ScrollWheelEvent {
            position: gpui::point(px(0.), px(0.)),
            delta: ScrollDelta::Lines(gpui::point(1.0, -3.0)),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Started,
        };
        let (dx, dy) = scroll_delta(&e);
        assert_eq!(dx, 40.0);
        assert_eq!(dy, -120.0);
    }

    #[test]
    fn 像素滚轮原样透传() {
        let e = ScrollWheelEvent {
            position: gpui::point(px(0.), px(0.)),
            delta: ScrollDelta::Pixels(gpui::point(px(5.), px(-12.))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Started,
        };
        let (dx, dy) = scroll_delta(&e);
        assert_eq!(dx, 5.0);
        assert_eq!(dy, -12.0);
    }

    #[test]
    fn 共享_bounds_初始为空() {
        let slot = new_shared_bounds();
        assert!(slot.get().is_none());
    }
}
