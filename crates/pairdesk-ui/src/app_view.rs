//! 主视图：首页 / 被控端 / 控制端。
//!
//! # 职责边界（整洁架构）
//!
//! 本文件**只做三件事**：
//! 1. 把 `pairdesk_shell::SessionState` 渲染成像素
//! 2. 把用户操作转成 `pairdesk_shell` 的调用
//! 3. 持有 UI 本地状态（输入框内容、折叠面板开合）
//!
//! **不做**：会话阶段推导、坐标换算、按键映射、权限判定、凭据格式。
//! 那些全在 `pairdesk-shell` 里，有 88 个单测覆盖。

use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    switch::Switch, Disableable as _, Sizable as _, StyledExt as _,
};
use pairdesk_shell::credentials::CredentialPair;
use pairdesk_shell::input::InputMsg;
use pairdesk_shell::permissions::{self, PermissionBanner as Banner, RequiredFor};
use pairdesk_shell::session::{
    ConnectParams, ConnectionMode, HostParams, Intent, SessionPhase, SessionState,
};
use pairdesk_shell::update::UpdateInfo;

use crate::frame::FramePump;
use crate::session_host::{self, ActiveSession};
use crate::video_view::{self, SharedBounds};
use crate::theme::{size, Palette};

/// 默认中继地址。与 `app/src/constants.ts` 一致。
pub const DEFAULT_RELAY: &str = "35.209.148.235:8977";
/// 默认打洞端口。
pub const DEFAULT_HOLE_PORT: &str = "23517";

/// 当前页面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Host,
    Viewer,
}

/// 应用根视图。
pub struct PairDeskApp {
    // ---------- 导航 ----------
    page: Page,

    // ---------- 领域状态 ----------
    /// 会话状态机（业务真相）
    session: SessionState,
    /// 活动会话句柄
    active: Option<ActiveSession>,
    /// 画面帧泵
    pump: Option<FramePump>,

    // ---------- 被控端本地状态 ----------
    host_pair: CredentialPair,
    host_relay: Entity<InputState>,
    host_hole_port: Entity<InputState>,
    host_mode: ConnectionMode,
    host_advanced_open: bool,
    host_copied: bool,

    // ---------- 控制端本地状态 ----------
    viewer_sid: Entity<InputState>,
    viewer_password: Entity<InputState>,
    viewer_target: Entity<InputState>,
    viewer_mode: ConnectionMode,
    viewer_advanced_open: bool,

    // ---------- 权限 ----------
    perm_status: pairdesk_core::permissions::PermissionStatus,

    // ---------- 更新 ----------
    update_msg: Option<SharedString>,
    update_checking: bool,
    update_info: Option<UpdateInfo>,
    /// 应用内安装的进度文案（None 表示未在安装）
    update_progress: Option<SharedString>,

    // ---------- 画面区 ----------
    /// 画面元素的焦点句柄（键盘事件需要）
    video_focus: gpui::FocusHandle,
    /// 画面矩形（canvas 写入，鼠标换算读取）
    video_bounds: SharedBounds,
}

impl PairDeskApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let host_relay = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_RELAY));
        let host_hole_port =
            cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_HOLE_PORT));
        let viewer_sid = cx.new(|cx| InputState::new(window, cx).placeholder("如 pd-AB12C34"));
        let viewer_password = cx.new(|cx| InputState::new(window, cx).placeholder("一次性密码"));
        let viewer_target = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_RELAY));

        // 初始化输入框默认值
        host_relay.update(cx, |st, cx| st.set_value(DEFAULT_RELAY, window, cx));
        host_hole_port.update(cx, |st, cx| st.set_value(DEFAULT_HOLE_PORT, window, cx));
        viewer_target.update(cx, |st, cx| st.set_value(DEFAULT_RELAY, window, cx));

        // 权限检测：失败时兜底为"全都有"，不阻塞用户（封装在领域层）
        let perm_status = permissions::check_safe();

        // 自测入口：PD_START_PAGE=host|viewer 直接进入指定页面（UI 自动化验证用）
        let page = match std::env::var("PD_START_PAGE").as_deref() {
            Ok("host") => Page::Host,
            Ok("viewer") => Page::Viewer,
            _ => Page::Home,
        };

        // 自测：PD_AUTOSTART=host|viewer 时跳过 UI 交互直接起会话
        // （用于无头端到端验证；真实用户路径仍走按钮点击）
        let autostart = std::env::var("PD_AUTOSTART").ok();

        let mut app = Self {
            page,
            session: SessionState::default(),
            active: None,
            pump: None,
            host_pair: CredentialPair::generate(),
            host_relay,
            host_hole_port,
            host_mode: ConnectionMode::Relay,
            host_advanced_open: false,
            host_copied: false,
            viewer_sid,
            viewer_password,
            viewer_target,
            viewer_mode: ConnectionMode::Relay,
            viewer_advanced_open: false,
            perm_status,
            update_msg: None,
            update_checking: false,
            update_info: None,
            update_progress: None,
            video_focus: cx.focus_handle(),
            video_bounds: video_view::new_shared_bounds(),
        };

        if let Some(role) = autostart {
            app.prepare_autostart(role, window, cx);
        }
        app
    }

    /// 自测入口：按角色预设参数并直接发起会话。
    fn prepare_autostart(&mut self, role: String, window: &mut Window, cx: &mut Context<Self>) {
        let sid = std::env::var("PD_SID").unwrap_or_else(|_| "pd-SELFTEST".to_string());
        let password = std::env::var("PD_PASSWORD").unwrap_or_else(|_| "123456".to_string());
        let relay = std::env::var("PD_RELAY").unwrap_or_else(|_| DEFAULT_RELAY.to_string());
        let hole_port = std::env::var("PD_HOLE").unwrap_or_else(|_| DEFAULT_HOLE_PORT.to_string());

        // 被控端需要「允许远程控制」开关打开，否则状态机会拒绝
        self.session.set_allowed(true);

        match role.as_str() {
            "host" => {
                self.page = Page::Host;
                self.host_pair = CredentialPair { sid, password };
                self.host_mode = ConnectionMode::Auto;
                self.host_relay.update(cx, |st, cx| st.set_value(relay, window, cx));
                self.host_hole_port.update(cx, |st, cx| st.set_value(hole_port, window, cx));
                self.host_start(cx);
            }
            "viewer" => {
                self.page = Page::Viewer;
                self.viewer_mode = ConnectionMode::Auto;
                self.viewer_sid.update(cx, |st, cx| st.set_value(sid, window, cx));
                self.viewer_password.update(cx, |st, cx| st.set_value(password, window, cx));
                self.viewer_target.update(cx, |st, cx| st.set_value(relay, window, cx));
                self.viewer_connect(cx);
            }
            _ => {}
        }
    }

    fn palette(&self, window: &Window) -> Palette {
        Palette::of(crate::frame::theme_mode(window))
    }

    /// 执行状态机返回的意图。
    ///
    /// 这是整个适配器层唯一需要"真正干活"的地方：把 `Intent` 交给
    /// `session_host` 去起线程、持句柄。
    fn dispatch(&mut self, intent: Intent, cx: &mut Context<Self>) {
        match intent {
            Intent::StartHost(_) | Intent::Connect(_) => {
                // 先停掉旧会话
                if let Some(old) = self.active.take() {
                    old.stop();
                }
                self.pump = None;

                match session_host::start(intent) {
                    Ok(mut session) => {
                        // 启动帧泵（关键：用 cx.spawn 定时器，不在 render 里 notify）
                        if let Some(rx) = session.take_frame_rx() {
                            self.pump = Some(FramePump::new(rx));
                        }
                        self.active = Some(session);
                        self.start_event_pump(cx);
                        self.start_frame_pump(cx);
                    }
                    Err(e) => {
                        self.session.phase = SessionPhase::Error;
                        self.session.error = Some(e);
                        cx.notify();
                    }
                }
            }
            Intent::Stop => {
                if let Some(s) = self.active.take() {
                    s.stop();
                }
                self.pump = None;
                cx.notify();
            }
            Intent::SendInput(msg) => {
                if let Some(s) = &self.active {
                    session_host::send_input(s, msg);
                }
            }
            Intent::None => {}
        }
    }

    /// 启动状态事件泵：把内核事件喂给状态机。
    fn start_event_pump(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let Ok(more) = this.update(cx, |app, cx| {
                    let Some(session) = &app.active else {
                        return false;
                    };
                    let mut changed = false;
                    // 一次最多消费 32 个事件，避免饿死渲染
                    for _ in 0..32 {
                        let Ok(ev) = session.event_rx.try_recv() else {
                            break;
                        };
                        if app.session.apply(&ev) {
                            changed = true;
                        }
                    }
                    if changed {
                        cx.notify();
                    }
                    true
                }) else {
                    break;
                };
                if !more {
                    break;
                }
            }
        })
        .detach();
    }

    /// 启动画面帧泵。
    ///
    /// 关键约束（实测）：必须用 `cx.spawn` + timer 驱动，
    /// 在 `render()` 里 `cx.notify()` 会导致帧率塌到 0.2fps。
    fn start_frame_pump(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(15))
                    .await;
                let Ok(keep) = this.update(cx, |app, cx| {
                    let Some(pump) = &mut app.pump else {
                        return false;
                    };
                    if pump.try_advance(cx) {
                        cx.notify();
                    }
                    true
                }) else {
                    break;
                };
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    // ---------- 被控端操作 ----------

    fn host_start(&mut self, cx: &mut Context<Self>) {
        let relay = self.host_relay.read(cx).value().to_string();
        let hole_port: u16 = self
            .host_hole_port
            .read(cx)
            .value()
            .parse()
            .unwrap_or(23517);

        let params = HostParams {
            mode: self.host_mode,
            relay,
            sid: self.host_pair.sid.clone(),
            hole_port,
            password: self.host_pair.password.clone(),
        };

        match self.session.start_host(params) {
            Ok(intent) => self.dispatch(intent, cx),
            Err(reason) => {
                self.session.error = Some(reason.message().to_string());
                cx.notify();
            }
        }
    }

    fn host_disconnect(&mut self, cx: &mut Context<Self>) {
        let intent = self.session.disconnect();
        self.dispatch(intent, cx);
    }

    fn host_regenerate(&mut self, cx: &mut Context<Self>) {
        self.host_pair = CredentialPair::generate();
        cx.notify();
    }

    fn host_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.host_pair.share_text();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.host_copied = true;
        cx.notify();

        // 1.4 秒后恢复按钮文案（与旧实现一致）
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1400))
                .await;
            this.update(cx, |app, cx| {
                app.host_copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();

        let _ = window;
    }

    /// 应用内下载并安装更新（对应旧 `pd_install_update`）。
    ///
    /// 下载完成后调平台安装脚本并退出当前进程——安装脚本会重启新版。
    fn start_install(&mut self, url: String, digest: Option<String>, cx: &mut Context<Self>) {
        if self.update_progress.is_some() {
            return; // 已在下载中
        }
        self.update_progress = Some(SharedString::from("准备下载…"));
        cx.notify();

        // 进度节流：每 5% 才推一次 UI，避免每个 chunk 都 notify 造成重绘风暴
        let last_percent = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        // 跨线程安全的进度中转：下载在 background executor 上跑，
        // 回调里不能直接碰 Context，所以写入共享槽，再由轮询任务读出来更新 UI。
        let progress_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<u32>));

        // 轮询任务：把下载进度反映到界面上
        let poll_slot = progress_slot.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                let latest = *poll_slot.lock().unwrap();
                let Ok(keep_going) = this.update(cx, |app, cx| {
                    match latest {
                        Some(pct) => {
                            app.update_progress =
                                Some(SharedString::from(format!("正在下载… {pct}%")));
                            cx.notify();
                        }
                        None => {
                            // None 表示下载已结束（成功或失败），由主流程接管后续文案
                            return false;
                        }
                    }
                    true
                }) else {
                    return;
                };
                if !keep_going {
                    return;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let result = crate::update::download_installer(&url, digest.as_deref(), {
                let slot = progress_slot.clone();
                let last = last_percent.clone();
                move |p| {
                    let mut last = last.borrow_mut();
                    if p.percent >= *last + 5 || p.percent == 100 {
                        *last = p.percent;
                        *slot.lock().unwrap() = Some(p.percent);
                    }
                }
            })
            .await;

            // 标记下载结束，让轮询任务退出
            *progress_slot.lock().unwrap() = None;

            match result {
                Ok(path) => {
                    this.update(cx, |app, cx| {
                        app.update_progress = Some(SharedString::from("下载完成，正在安装…"));
                        cx.notify();
                    })
                    .ok();

                    match crate::update::install_and_exit(&path) {
                        Ok(()) => {
                            // 安装脚本会重启新版，当前进程退出
                            this.update(cx, |_, cx| cx.quit()).ok();
                        }
                        Err(e) => {
                            this.update(cx, |app, cx| {
                                app.update_progress =
                                    Some(SharedString::from(format!("安装失败: {e}")));
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                }
                Err(e) => {
                    this.update(cx, |app, cx| {
                        app.update_progress = Some(SharedString::from(format!("下载失败: {e}")));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// 复制任意文本到系统剪贴板。
    fn copy_text(&mut self, text: String, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        cx.notify();
    }

    fn toggle_allowed(&mut self, allowed: bool, cx: &mut Context<Self>) {
        self.session.set_allowed(allowed);
        cx.notify();
    }

    // ---------- 控制端操作 ----------

    fn viewer_connect(&mut self, cx: &mut Context<Self>) {
        let sid = self.viewer_sid.read(cx).value().to_string();
        let password = self.viewer_password.read(cx).value().to_string();
        let target = self.viewer_target.read(cx).value().to_string();

        let params = ConnectParams {
            mode: self.viewer_mode,
            target,
            sid,
            password,
        };

        match self.session.connect(params) {
            Ok(intent) => self.dispatch(intent, cx),
            Err(reason) => {
                self.session.error = Some(reason.message().to_string());
                cx.notify();
            }
        }
    }

    fn viewer_disconnect(&mut self, cx: &mut Context<Self>) {
        let intent = self.session.disconnect();
        self.dispatch(intent, cx);
    }

    // ---------- 更新 ----------

    fn check_update(&mut self, cx: &mut Context<Self>) {
        self.update_checking = true;
        self.update_msg = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = crate::update::check().await;
            this.update(cx, |app, cx| {
                app.update_checking = false;
                match result {
                    Ok(info) if info.has_update => {
                        app.update_msg =
                            Some(SharedString::from(format!("发现新版本 v{}", info.latest_version)));
                        // 保存详情，渲染更新弹窗
                        app.update_info = Some(info);
                    }
                    Ok(info) => {
                        app.update_msg = Some(SharedString::from(format!(
                            "当前已是最新版本 (v{})",
                            info.current_version
                        )));
                    }
                    Err(e) => {
                        app.update_msg = Some(SharedString::from(format!("检查更新失败: {e}")));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---------- 渲染 ----------

    fn render_home(&self, pal: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let banner = permissions::banner_for(&self.perm_status, RequiredFor::Both);

        div()
            .v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .p(px(48.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_semibold()
                    .text_color(pal.fg)
                    .child("PairDesk"),
            )
            .child(
                div()
                    .text_size(size::text_xl())
                    .text_color(pal.fg)
                    .child("两台电脑，一次远程协助"),
            )
            .child(
                div()
                    .text_size(size::text_md())
                    .text_color(pal.fg_muted)
                    .child("同网直连 · 跨网打洞 · 选择你的角色"),
            )
            .when_some(banner, |this, b| {
                this.child(self.render_permission_banner(&b, pal, cx))
            })
            .child(
                div()
                    .h_flex()
                    .gap_4()
                    .mt(px(32.))
                    .child(self.render_mode_card(
                        "被控端",
                        "分享屏幕，让对方操作这台电脑",
                        Page::Host,
                        pal,
                        cx,
                    ))
                    .child(self.render_mode_card(
                        "控制端",
                        "输入会话码，远程操作另一台电脑",
                        Page::Viewer,
                        pal,
                        cx,
                    )),
            )
            .child(
                div()
                    .v_flex()
                    .items_center()
                    .gap_2()
                    .mt(px(40.))
                    .child(
                        div()
                            .id("check-update")
                            .text_size(px(12.))
                            .text_color(pal.fg_muted)
                            .cursor_pointer()
                            .hover(|s| s.text_color(pal.fg))
                            .on_click(cx.listener(|app, _, _, cx| {
                                if !app.update_checking {
                                    app.check_update(cx);
                                }
                            }))
                            .child(if self.update_checking {
                                "正在检查更新…"
                            } else {
                                "检查更新"
                            }),
                    )
                    .when_some(self.update_msg.clone(), |this, msg| {
                        this.child(div().text_size(px(12.)).text_color(pal.primary).child(msg))
                    }),
            )
            .when_some(self.update_info.clone(), |this, info| {
                this.child(self.render_update_modal(&info, pal, cx))
            })
    }

    /// 更新弹窗：显示版本与更新说明，提供「前往下载」与「关闭」。
    fn render_update_modal(
        &self,
        info: &UpdateInfo,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let notes = if info.release_notes.is_empty() {
            "（该版本没有提供更新说明）".to_string()
        } else {
            // 只展示前 800 字符，避免长文撑爆窗口
            let trimmed: String = info.release_notes.chars().take(800).collect();
            trimmed
        };
        let url = info.download_url.clone();
        let url_browser = url.clone();
        let digest = info.digest.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000099))
            .child(
                div()
                    .v_flex()
                    .w(px(460.))
                    .gap_4()
                    .p(px(24.))
                    .rounded(size::radius_lg())
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.bg_elev)
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_semibold()
                            .text_color(pal.fg)
                            .child(format!("发现新版本 v{}", info.latest_version)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(pal.fg_muted)
                            .child(format!(
                                "当前版本 v{} → 最新版本 v{}",
                                info.current_version, info.latest_version
                            )),
                    )
                    .child(
                        div()
                            .max_h(px(240.))
                            .overflow_hidden()
                            .text_size(size::text_sm())
                            .text_color(pal.fg_muted)
                            .child(notes),
                    )
                    .when_some(self.update_progress.clone(), |this, p| {
                        this.child(
                            div()
                                .text_size(px(12.))
                                .text_color(pal.primary)
                                .child(p),
                        )
                    })
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("update-close")
                                    .small()
                                    .label("稍后再说")
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.update_info = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("update-browser")
                                    .small()
                                    .label("前往下载")
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        // 打开系统浏览器（无应用内安装权限时的兜底）
                                        cx.open_url(&url_browser);
                                        app.update_info = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("update-install")
                                    .primary()
                                    .small()
                                    .label("立即安装")
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.start_install(url.clone(), digest.clone(), cx);
                                    })),
                            ),
                    ),
            )
    }

    fn render_mode_card(
        &self,
        title: &'static str,
        desc: &'static str,
        target: Page,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .v_flex()
            .w(px(260.))
            .p(px(24.))
            .gap_2()
            .rounded(size::radius_lg())
            .border_1()
            .border_color(pal.border)
            .bg(pal.bg_elev)
            .cursor_pointer()
            .hover(|s| s.border_color(pal.primary).bg(pal.bg_hover))
            .id(("mode-card", target as usize))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.page = target;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(16.))
                    .font_semibold()
                    .text_color(pal.fg)
                    .child(title),
            )
            .child(
                div()
                    .text_size(size::text_sm())
                    .text_color(pal.fg_muted)
                    .child(desc),
            )
    }

    fn render_permission_banner(
        &self,
        banner: &Banner,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let missing = banner.missing.clone();
        // 每个按钮闭包需要独立的副本（闭包是 move 的）
        let missing_request = missing.clone();
        let missing_open = missing.clone();
        let kinds: Vec<&'static str> = missing.iter().map(|k| k.label()).collect();

        div()
            .v_flex()
            .w_full()
            .max_w(px(560.))
            .gap_2()
            .p(px(16.))
            .mt(px(24.))
            .rounded(size::radius_lg())
            .border_1()
            .border_color(pal.warn)
            .child(
                div()
                    .text_size(px(14.))
                    .font_semibold()
                    .text_color(pal.fg)
                    .child(banner.title()),
            )
            .child(
                div()
                    .text_size(size::text_sm())
                    .text_color(pal.fg_muted)
                    .child(format!("缺少权限：{}", kinds.join("、"))),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        Button::new("perm-request")
                            .small()
                            .primary()
                            .label("请求授权")
                            .on_click(cx.listener(move |app, _, _, cx| {
                                // 触发 macOS 系统权限弹窗（屏幕录制会弹系统对话框）
                                if let Some(kind) = missing_request.first() {
                                    permissions::request(*kind);
                                }
                                // 稍后重新检测（用户可能已授权）
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor()
                                        .timer(std::time::Duration::from_millis(1000))
                                        .await;
                                    this.update(cx, |app, cx| {
                                        app.perm_status = permissions::check_safe();
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .detach();
                                let _ = app;
                            })),
                    )
                    .child(
                        Button::new("perm-open")
                            .small()
                            .label("打开系统设置")
                            .on_click(cx.listener(move |_app, _, _, _cx| {
                                // 打开第一个缺失权限对应的系统设置面板
                                if let Some(kind) = missing_open.first() {
                                    permissions::open_settings(*kind);
                                }
                            })),
                    )
                    .child(
                        Button::new("perm-recheck")
                            .small()
                            .label("重新检测")
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.perm_status = permissions::check_safe();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("perm-restart")
                            .small()
                            .label("重启应用")
                            .on_click(cx.listener(|_app, _, _, _cx| {
                                // macOS 的 TCC 权限按进程缓存，授权后必须重启才能读到新状态。
                                // restart_app 会 spawn 新进程后 exit，不再返回。
                                permissions::restart_app();
                            })),
                    ),
            )
    }

    fn render_page_header(
        &self,
        title: &'static str,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let phase = self.session.phase;
        let tone = pal.status(phase.tone());
        let label = if self.page == Page::Host {
            phase.host_label()
        } else {
            phase.viewer_label()
        };

        div()
            .h_flex()
            .items_center()
            .gap_3()
            .w_full()
            .child(
                div()
                    .text_size(size::text_sm())
                    .text_color(pal.fg_muted)
                    .cursor_pointer()
                    .hover(|s| s.text_color(pal.fg))
                    .id("back-home")
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.page = Page::Home;
                        cx.notify();
                    }))
                    .child("← 返回"),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(16.))
                    .font_semibold()
                    .text_color(pal.fg)
                    .child(title),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().size(px(8.)).rounded_full().bg(tone))
                    .child(
                        div()
                            .text_size(size::text_sm())
                            .text_color(pal.fg_muted)
                            .child(label),
                    ),
            )
    }

    fn render_host(&self, pal: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.session.phase.is_active();
        let banner = permissions::banner_for(&self.perm_status, RequiredFor::Host);
        let allowed = self.session.allowed;
        let can_start = self.host_pair.is_complete() && allowed;

        let body = if active {
            self.render_host_active(pal, cx).into_any_element()
        } else {
            self.render_host_form(pal, can_start, cx).into_any_element()
        };

        div()
            .v_flex()
            .size_full()
            .p(size::page_pad())
            .gap_4()
            .child(self.render_page_header("被控端", pal, cx))
            .when_some(banner, |this, b| {
                this.child(self.render_permission_banner(&b, pal, cx))
            })
            .child(body)
    }

    fn render_host_form(
        &self,
        pal: Palette,
        can_start: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sid = self.host_pair.sid.clone();
        let password = self.host_pair.password.clone();
        let allowed = self.session.allowed;
        let mode = self.host_mode;
        let advanced_open = self.host_advanced_open;

        div()
            .v_flex()
            .gap_4()
            .max_w(size::form_max_width())
            .child(
                div()
                    .text_size(size::text_sm())
                    .text_color(pal.fg_muted)
                    .child("把会话码和密码发给对方，打开开关后即可等待连入。"),
            )
            // 凭据卡片
            .child(
                div()
                    .v_flex()
                    .gap_4()
                    .p(px(16.))
                    .rounded(size::radius_md())
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.bg_elev)
                    .child(self.render_credential("会话码", sid.clone(), pal, cx))
                    .child(self.render_credential("连接密码", password.clone(), pal, cx)),
            )
            // 允许开关
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .p(px(16.))
                    .rounded(size::radius_lg())
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.bg_elev)
                    .child(
                        div()
                            .v_flex()
                            .child(
                                div()
                                    .text_size(size::text_sm())
                                    .text_color(pal.fg)
                                    .child("允许远程控制"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(pal.fg_muted)
                                    .child("关闭后即使有码也无法连入"),
                            ),
                    )
                    .child(
                        Switch::new("allow")
                            .checked(allowed)
                            .on_click(cx.listener(|app, v: &bool, _, cx| {
                                app.toggle_allowed(*v, cx);
                            })),
                    ),
            )
            // 主按钮
            .child(
                Button::new("start-host")
                    .primary()
                    .large()
                    .w_full()
                    .label("开始接收协助")
                    .disabled(!can_start)
                    .on_click(cx.listener(|app, _, _, cx| app.host_start(cx))),
            )
            // 高级设置
            .child(
                div()
                    .v_flex()
                    .rounded(size::radius_md())
                    .border_1()
                    .border_color(pal.border)
                    .child(
                        div()
                            .p(px(12.))
                            .text_size(size::text_sm())
                            .text_color(pal.fg_muted)
                            .cursor_pointer()
                            .hover(|s| s.text_color(pal.fg))
                            .id("host-advanced")
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.host_advanced_open = !app.host_advanced_open;
                                cx.notify();
                            }))
                            .child(format!(
                                "{} 高级设置",
                                if advanced_open { "▾" } else { "▸" }
                            )),
                    )
                    .when(advanced_open, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap_3()
                                .px(px(12.))
                                .pb(px(12.))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(pal.fg_muted)
                                        .child(format!("连接模式：{}", mode.as_str())),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .gap_2()
                                        .child(self.render_mode_button(
                                            "中继", ConnectionMode::Relay, true, pal, cx,
                                        ))
                                        .child(self.render_mode_button(
                                            "QUIC", ConnectionMode::Quic, true, pal, cx,
                                        ))
                                        .child(self.render_mode_button(
                                            "直连", ConnectionMode::Direct, true, pal, cx,
                                        ))
                                        .child(self.render_mode_button(
                                            "自动", ConnectionMode::Auto, true, pal, cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(pal.fg_muted)
                                                .child("中继/VPS 地址"),
                                        )
                                        .child(Input::new(&self.host_relay).h(px(36.))),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(pal.fg_muted)
                                                .child("打洞/直连端口"),
                                        )
                                        .child(Input::new(&self.host_hole_port).h(px(36.))),
                                ),
                        )
                    }),
            )
            .when_some(self.session.notice.clone(), |this, n| {
                this.child(
                    div()
                        .p(px(12.))
                        .rounded(size::radius_md())
                        .bg(pal.primary_soft)
                        .text_size(size::text_sm())
                        .text_color(pal.primary)
                        .child(n),
                )
            })
            .when_some(self.session.error.clone(), |this, e| {
                this.child(
                    div()
                        .p(px(12.))
                        .rounded(size::radius_md())
                        .border_1()
                        .border_color(pal.danger)
                        .text_size(size::text_sm())
                        .text_color(pal.danger)
                        .child(e),
                )
            })
    }

    fn render_mode_button(
        &self,
        label: &'static str,
        mode: ConnectionMode,
        is_host: bool,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current = if is_host {
            self.host_mode
        } else {
            self.viewer_mode
        };
        let selected = current == mode;

        div()
            .px(px(12.))
            .py(px(6.))
            .rounded(size::radius_sm())
            .border_1()
            .border_color(if selected { pal.primary } else { pal.border })
            .bg(if selected { pal.primary_soft } else { pal.bg_elev })
            .text_size(px(12.))
            .text_color(if selected { pal.primary } else { pal.fg_muted })
            .cursor_pointer()
            .id(SharedString::from(format!("mode-btn-{}-{is_host}", mode.as_str())))
            .on_click(cx.listener(move |app, _, _, cx| {
                if is_host {
                    app.host_mode = mode;
                } else {
                    app.viewer_mode = mode;
                }
                cx.notify();
            }))
            .child(label)
    }

    fn render_credential(
        &self,
        label: &'static str,
        value: String,
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(size::text_sm())
                    .text_color(pal.fg_muted)
                    .child(label),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .h(size::control_lg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(size::radius_md())
                            .border_1()
                            .border_color(pal.border)
                            .bg(pal.bg_input)
                            .text_size(size::text_credential())
                            .font_semibold()
                            .text_color(pal.fg)
                            .child(value.to_string()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("copy-{label}")))
                            .ghost()
                            .small()
                            .label("复制")
                            .on_click(cx.listener({
                                let value = value.clone();
                                move |app, _, window, cx| {
                                    app.copy_text(value.clone(), window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("regen-{label}")))
                            .ghost()
                            .small()
                            .label("换")
                            .on_click(cx.listener(|app, _, _, cx| app.host_regenerate(cx))),
                    ),
            )
    }

    fn render_host_active(&self, pal: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let sid = self.host_pair.sid.clone();
        let password = self.host_pair.password.clone();
        let phase = self.session.phase;
        let copied = self.host_copied;
        let transport = self.session.transport.clone();
        let notice = self.session.notice.clone();

        div()
            .v_flex()
            .items_center()
            .gap_3()
            .max_w(size::form_max_width())
            .child(
                div()
                    .v_flex()
                    .w_full()
                    .items_center()
                    .gap_4()
                    .p(px(20.))
                    .rounded(size::radius_lg())
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.bg_elev)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(pal.fg_muted)
                            .child(if phase == SessionPhase::Connected {
                                "正在被远程控制"
                            } else {
                                "正在等待对方连接"
                            }),
                    )
                    .child(
                        div()
                            .v_flex()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .font_semibold()
                                    .text_color(pal.fg)
                                    .child(sid),
                            )
                            .child(
                                div()
                                    .text_size(size::text_sm())
                                    .text_color(pal.fg_muted)
                                    .child("会话码"),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .items_center()
                            .child(
                                div()
                                    .text_size(size::text_password())
                                    .font_semibold()
                                    .text_color(pal.primary)
                                    .child(password),
                            )
                            .child(
                                div()
                                    .text_size(size::text_sm())
                                    .text_color(pal.fg_muted)
                                    .child("连接密码"),
                            ),
                    )
                    .child(
                        Button::new("copy-share")
                            .w_full()
                            .label(if copied { "已复制" } else { "复制会话信息" })
                            .on_click(cx.listener(|app, _, window, cx| app.host_copy(window, cx))),
                    )
                    .child(
                        Button::new("disconnect")
                            .danger()
                            .w_full()
                            .label("断开连接")
                            .on_click(cx.listener(|app, _, _, cx| app.host_disconnect(cx))),
                    ),
            )
            .when_some(transport, |this, t| {
                this.child(
                    div()
                        .text_size(size::text_sm())
                        .text_color(pal.fg_muted)
                        .child(format!("传输路径：{t}")),
                )
            })
            .when_some(notice, |this, n| {
                this.child(
                    div()
                        .p(px(12.))
                        .rounded(size::radius_md())
                        .bg(pal.primary_soft)
                        .text_size(size::text_sm())
                        .text_color(pal.primary)
                        .child(n),
                )
            })
    }

    fn render_viewer(&self, pal: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = matches!(
            self.session.phase,
            SessionPhase::Connected | SessionPhase::Authenticated
        );
        let banner = permissions::banner_for(&self.perm_status, RequiredFor::Viewer);
        let busy = self.session.phase == SessionPhase::Authentication;

        if connected {
            return self.render_viewer_connected(pal, cx).into_any_element();
        }

        let sid = self.viewer_sid.read(cx).value().to_string();
        let password = self.viewer_password.read(cx).value().to_string();
        let can_connect = !sid.is_empty() && !password.is_empty() && !busy;
        let mode = self.viewer_mode;
        let advanced_open = self.viewer_advanced_open;
        let target_label = if mode.needs_relay_address() {
            "中继/VPS 地址"
        } else {
            "对端目标地址 (IP:端口)"
        };

        div()
            .v_flex()
            .size_full()
            .p(size::page_pad())
            .gap_4()
            .child(self.render_page_header("控制端", pal, cx))
            .when_some(banner, |this, b| {
                this.child(self.render_permission_banner(&b, pal, cx))
            })
            .child(
                div()
                    .v_flex()
                    .gap_4()
                    .max_w(size::form_max_width())
                    .child(
                        div()
                            .text_size(size::text_sm())
                            .text_color(pal.fg_muted)
                            .child("输入对方给的会话码和密码，点击连接开始协助。"),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_4()
                            .p(px(16.))
                            .rounded(size::radius_md())
                            .border_1()
                            .border_color(pal.border)
                            .bg(pal.bg_elev)
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(size::text_sm())
                                            .text_color(pal.fg_muted)
                                            .child("对方会话码"),
                                    )
                                    .child(Input::new(&self.viewer_sid).h(size::control())),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(size::text_sm())
                                            .text_color(pal.fg_muted)
                                            .child("连接密码"),
                                    )
                                    .child(Input::new(&self.viewer_password).h(size::control())),
                            ),
                    )
                    .child(
                        Button::new("connect")
                            .primary()
                            .large()
                            .w_full()
                            .label(if busy { "连接中…" } else { "连接" })
                            .disabled(!can_connect)
                            .on_click(cx.listener(|app, _, _, cx| app.viewer_connect(cx))),
                    )
                    .child(
                        div()
                            .v_flex()
                            .rounded(size::radius_md())
                            .border_1()
                            .border_color(pal.border)
                            .child(
                                div()
                                    .p(px(12.))
                                    .text_size(size::text_sm())
                                    .text_color(pal.fg_muted)
                                    .cursor_pointer()
                                    .id("viewer-advanced")
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.viewer_advanced_open = !app.viewer_advanced_open;
                                        cx.notify();
                                    }))
                                    .child(format!(
                                        "{} 高级设置",
                                        if advanced_open { "▾" } else { "▸" }
                                    )),
                            )
                            .when(advanced_open, |this| {
                                this.child(
                                    div()
                                        .v_flex()
                                        .gap_3()
                                        .px(px(12.))
                                        .pb(px(12.))
                                        .child(
                                            div()
                                                .h_flex()
                                                .gap_2()
                                                .child(self.render_mode_button(
                                                    "中继",
                                                    ConnectionMode::Relay,
                                                    false,
                                                    pal,
                                                    cx,
                                                ))
                                                .child(self.render_mode_button(
                                                    "QUIC",
                                                    ConnectionMode::Quic,
                                                    false,
                                                    pal,
                                                    cx,
                                                ))
                                                .child(self.render_mode_button(
                                                    "直连",
                                                    ConnectionMode::Direct,
                                                    false,
                                                    pal,
                                                    cx,
                                                ))
                                                .child(self.render_mode_button(
                                                    "自动",
                                                    ConnectionMode::Auto,
                                                    false,
                                                    pal,
                                                    cx,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .v_flex()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .text_color(pal.fg_muted)
                                                        .child(target_label),
                                                )
                                                .child(Input::new(&self.viewer_target).h(px(36.))),
                                        ),
                                )
                            }),
                    )
                    .when_some(self.session.error.clone(), |this, e| {
                        this.child(
                            div()
                                .p(px(12.))
                                .rounded(size::radius_md())
                                .border_1()
                                .border_color(pal.danger)
                                .text_size(size::text_sm())
                                .text_color(pal.danger)
                                .child(e),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_viewer_connected(&self, pal: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let sid = self.viewer_sid.read(cx).value().to_string();
        let transport = self.session.transport.clone();
        let image = self.pump.as_ref().and_then(|p| p.image());
        // 远端分辨率来自状态机（内核 Size 事件），未知时用 16:9 占位
        let remote_size = self.session.screen.unwrap_or((1920, 1080));

        // 输入转发：捕获到的 InputMsg 经状态机判定后投递给内核。
        // 用 Rc<dyn Fn> 是因为 video_view 需要多次克隆同一个处理器。
        let entity = cx.entity();
        let on_input: std::rc::Rc<dyn Fn(InputMsg, &mut Window, &mut gpui::App)> =
            std::rc::Rc::new(move |msg, _window, cx| {
                entity.update(cx, |app, cx| {
                    // 状态机决定是否真的转发（仅在 Connected 阶段）
                    if let Intent::SendInput(m) = app.session.send_input(msg) {
                        if let Some(s) = &app.active {
                            session_host::send_input(s, m);
                        }
                    }
                    let _ = cx;
                });
            });

        div()
            .v_flex()
            .size_full()
            .p(px(14.))
            .gap_3()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .p(px(8.))
                    .rounded(size::radius_md())
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.bg_elev)
                    .child(
                        div()
                            .flex_1()
                            .text_size(size::text_sm())
                            .font_semibold()
                            .text_color(pal.fg)
                            .child(sid),
                    )
                    .when_some(self.session.screen, |this, (w, h)| {
                        this.child(
                            div()
                                .text_size(px(12.))
                                .text_color(pal.fg_muted)
                                .child(format!("{w}×{h}")),
                        )
                    })
                    .child(
                        div()
                            .text_size(size::text_sm())
                            .text_color(pal.fg_muted)
                            .child(transport.unwrap_or_default()),
                    )
                    .child(
                        Button::new("disconnect-viewer")
                            .danger()
                            .small()
                            .label("断开")
                            .on_click(cx.listener(|app, _, _, cx| app.viewer_disconnect(cx))),
                    ),
            )
            .child(video_view::video_view(
                image,
                remote_size,
                self.video_bounds.clone(),
                self.video_focus.clone(),
                pal.video_bg,
                pal.fg_muted,
                "等待画面…",
                on_input,
            ))
    }
}

impl Render for PairDeskApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = self.palette(window);

        let content = match self.page {
            Page::Home => self.render_home(pal, cx).into_any_element(),
            Page::Host => self.render_host(pal, cx).into_any_element(),
            Page::Viewer => self.render_viewer(pal, cx).into_any_element(),
        };

        div()
            .v_flex()
            .size_full()
            .bg(pal.bg)
            .text_color(pal.fg)
            .text_size(size::text_md())
            .child(content)
    }
}
