//! 会话状态机。
//!
//! 行为来源（迁移前分散在两处，这里合并并显式化）：
//!
//! - `app/src/state/useSession.ts`：`SessionPhase` 定义与事件驱动的阶段转换
//! - `crates/pairdesk-app/src/bridge.rs`：命令校验（允许开关）、默认画质、错误文案
//!
//! 设计要点：本模块**不做 I/O**。它只回答两个问题：
//!
//! 1. 「用户点了这个按钮，该做什么？」→ 返回 [`Intent`]
//! 2. 「内核推来这个事件，界面该变成什么样？」→ 更新 [`SessionState`]
//!
//! 真正执行（起 socket、起线程）由适配器层按 [`Intent`] 去调 `pairdesk-core`。
//! 这样状态机本身可以在 `cargo test` 里被穷举验证，不需要真实网络。

use pairdesk_core::CoreEvent;

use crate::input::InputMsg;

/// 会话阶段。与 `useSession.ts` 的 `SessionPhase` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    /// 未开始
    Idle,
    /// 连接中 / 认证中
    Authentication,
    /// 已认证，等待首帧
    Authenticated,
    /// 已连接（画面在流）
    Connected,
    /// 已断开
    Disconnected,
    /// 出错
    Error,
}

impl SessionPhase {
    /// 是否处于"会话活跃"状态——被控端/控制端页面据此切换表单视图与画面视图。
    ///
    /// 原实现（`HostPage.tsx`）：
    /// ```js
    /// const active = phase === 'connected' || phase === 'authenticated' || phase === 'authentication';
    /// ```
    pub fn is_active(self) -> bool {
        matches!(
            self,
            SessionPhase::Authentication | SessionPhase::Authenticated | SessionPhase::Connected
        )
    }

    /// 是否应该显示"连接中"的脉冲动画。
    ///
    /// 原实现：`pulse={phase === 'authentication' || phase === 'authenticated'}`
    pub fn is_pending(self) -> bool {
        matches!(self, SessionPhase::Authentication | SessionPhase::Authenticated)
    }
}

/// 状态点色调。与 `ui-kit` 的 `StatusTone` 对齐，但用枚举表达以脱离 UI 库。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Idle,
    Connecting,
    Online,
    Error,
}

impl SessionPhase {
    /// 阶段 → 色调映射。
    ///
    /// 原实现见 `HostPage.tsx` / `ViewerPage.tsx` 里的 `TONE` 常量表，
    /// 两处内容完全一致，这里合并为唯一真相。
    pub fn tone(self) -> StatusTone {
        match self {
            SessionPhase::Idle => StatusTone::Idle,
            SessionPhase::Authentication => StatusTone::Connecting,
            SessionPhase::Authenticated => StatusTone::Online,
            SessionPhase::Connected => StatusTone::Online,
            SessionPhase::Disconnected => StatusTone::Idle,
            SessionPhase::Error => StatusTone::Error,
        }
    }

    /// 被控端页的状态文案（`HostPage.tsx::statusLabel`）。
    pub fn host_label(self) -> &'static str {
        match self {
            SessionPhase::Connected => "有人正在观看",
            SessionPhase::Authentication | SessionPhase::Authenticated => "等待连接",
            SessionPhase::Error => "出错",
            SessionPhase::Idle | SessionPhase::Disconnected => "未开始",
        }
    }

    /// 控制端页的状态文案（`ViewerPage.tsx::labelOf`）。
    pub fn viewer_label(self) -> &'static str {
        match self {
            SessionPhase::Idle => "未连接",
            SessionPhase::Authentication => "连接中…",
            SessionPhase::Authenticated => "已认证，等待画面",
            SessionPhase::Connected => "已连接",
            SessionPhase::Disconnected => "已断开",
            SessionPhase::Error => "出错",
        }
    }
}

/// 连接模式。与 `bridge/types.ts` 的 `ConnectionMode` 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionMode {
    /// 经 VPS 中继（默认，最稳）
    #[default]
    Relay,
    /// QUIC 直连（异网打洞）
    Quic,
    /// TCP 直连（局域网）
    Direct,
    /// 智能自动探测（QUIC 优先 → 中继兜底）
    Auto,
}

impl ConnectionMode {
    /// 解析界面下拉框的值。未知值回落到默认（Relay），与原实现一致。
    pub fn parse(s: &str) -> Self {
        match s {
            "quic" => ConnectionMode::Quic,
            "direct" => ConnectionMode::Direct,
            "auto" => ConnectionMode::Auto,
            _ => ConnectionMode::Relay,
        }
    }

    /// 序列化为 `pairdesk-core` 接受的字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionMode::Relay => "relay",
            ConnectionMode::Quic => "quic",
            ConnectionMode::Direct => "direct",
            ConnectionMode::Auto => "auto",
        }
    }

    /// 该模式是否需要中继地址（否则输入框是"对端目标地址"）。
    ///
    /// 原实现：`HostPage` 里 `(mode === 'relay' || mode === 'auto') && <TextField 中继地址>`。
    pub fn needs_relay_address(self) -> bool {
        matches!(self, ConnectionMode::Relay | ConnectionMode::Auto)
    }
}

/// 启动被控端的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostParams {
    pub mode: ConnectionMode,
    /// 中继/VPS 地址（`relay`/`auto` 模式使用）
    pub relay: String,
    pub sid: String,
    pub hole_port: u16,
    pub password: String,
}

/// 启动控制端的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectParams {
    pub mode: ConnectionMode,
    /// 中继地址（relay/auto）或对端地址（quic/direct）
    pub target: String,
    pub sid: String,
    pub password: String,
}

/// 适配器层应当执行的动作。
///
/// 状态机只产出意图，不执行——这样状态机可单测，且同一套逻辑可被
/// Tauri 壳与 GPUI 壳各自实现。
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// 调用 `CoreHandle::start_host*`
    StartHost(HostParams),
    /// 调用 `CoreHandle::connect*`
    Connect(ConnectParams),
    /// 调用 `CoreHandle::stop`
    Stop,
    /// 调用 `CoreHandle::send_input`
    SendInput(InputMsg),
    /// 什么都不做（被拒绝 / 无意义操作）
    None,
}

/// 命令被拒绝的原因。适配器负责把文案渲染出来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// 「允许远程控制」总开关是关闭的
    NotAllowed,
    /// 会话码或密码为空
    MissingCredential,
    /// 已有会话在跑，不能重复启动
    AlreadyRunning,
}

impl RejectReason {
    /// 面向用户的中文文案。
    ///
    /// `NotAllowed` 的文案与 `bridge.rs:108` 逐字一致。
    pub fn message(self) -> &'static str {
        match self {
            RejectReason::NotAllowed => "「允许远程控制」开关是关闭的",
            RejectReason::MissingCredential => "请先填写会话码和密码",
            RejectReason::AlreadyRunning => "已有会话正在进行，请先断开",
        }
    }
}

/// 默认画质。原实现 `bridge.rs::pd_start_host` 硬编码 `Quality { jpeg: 80, fps: 20 }`。
pub const DEFAULT_QUALITY: pairdesk_core::Quality = pairdesk_core::Quality { jpeg: 80, fps: 20 };

/// 会话状态。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionState {
    pub phase: SessionPhase,
    /// 远端屏幕分辨率（收到 `Size` 事件后填充）
    pub screen: Option<(u32, u32)>,
    pub error: Option<String>,
    /// 非致命提示（如打洞端口被占自动顺延）
    pub notice: Option<String>,
    /// 自动择一选中的传输路径
    pub transport: Option<String>,
    /// 「允许远程控制」总开关（默认关）
    pub allowed: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            screen: None,
            error: None,
            notice: None,
            transport: None,
            allowed: false,
        }
    }
}

impl SessionState {
    /// 开始一次新的会话：清空上一轮的残留状态。
    ///
    /// 对应原实现中每个 `startHost`/`connect` 里的：
    /// ```js
    /// setState({ phase: 'authentication', screen: null, error: null, notice: null, transport: null });
    /// ```
    fn enter_authentication(&mut self) {
        self.phase = SessionPhase::Authentication;
        self.screen = None;
        self.error = None;
        self.notice = None;
        self.transport = None;
    }

    /// 设置「允许远程控制」开关。
    pub fn set_allowed(&mut self, allowed: bool) {
        self.allowed = allowed;
    }

    /// 用户点击「开始接收协助」。
    pub fn start_host(&mut self, params: HostParams) -> Result<Intent, RejectReason> {
        // 原实现：bridge.rs::pd_start_host 先查开关
        if !self.allowed {
            return Err(RejectReason::NotAllowed);
        }
        if params.sid.is_empty() || params.password.is_empty() {
            return Err(RejectReason::MissingCredential);
        }
        self.enter_authentication();
        Ok(Intent::StartHost(params))
    }

    /// 用户点击「连接」。
    pub fn connect(&mut self, params: ConnectParams) -> Result<Intent, RejectReason> {
        if params.sid.is_empty() || params.password.is_empty() {
            return Err(RejectReason::MissingCredential);
        }
        if self.phase.is_active() {
            return Err(RejectReason::AlreadyRunning);
        }
        self.enter_authentication();
        Ok(Intent::Connect(params))
    }

    /// 用户点击「断开连接」。
    ///
    /// 原实现 `useSession.ts::disconnect`：先 `bridge.stop()`，再把 phase 设为
    /// `'disconnected'`。
    pub fn disconnect(&mut self) -> Intent {
        self.phase = SessionPhase::Disconnected;
        Intent::Stop
    }

    /// 控制端把输入事件交给状态机转发。
    ///
    /// 原实现：`bridge.rs::pd_send_input` 在无活动会话时静默忽略。
    pub fn send_input(&mut self, msg: InputMsg) -> Intent {
        if self.phase == SessionPhase::Connected {
            Intent::SendInput(msg)
        } else {
            Intent::None
        }
    }

    /// 应用一个内核事件，返回是否需要适配器重绘。
    ///
    /// 转换规则逐条对应 `useSession.ts` 的 `switch (e.type)`。
    pub fn apply(&mut self, ev: &CoreEvent) -> bool {
        match ev {
            CoreEvent::AuthResult { ok, reason } => {
                if *ok {
                    self.phase = SessionPhase::Authenticated;
                    self.error = None;
                } else {
                    self.phase = SessionPhase::Error;
                    // 原实现：`e.reason ?? '认证失败'`
                    self.error = Some(reason.clone().unwrap_or_else(|| "认证失败".to_string()));
                }
                true
            }
            CoreEvent::PeerConnected => {
                self.phase = SessionPhase::Connected;
                true
            }
            CoreEvent::PeerDisconnected => {
                self.phase = SessionPhase::Disconnected;
                true
            }
            CoreEvent::Size { w, h } => {
                self.screen = Some((*w, *h));
                true
            }
            CoreEvent::Transport(path) => {
                self.transport = Some(path.clone());
                true
            }
            CoreEvent::Error(msg) => {
                self.phase = SessionPhase::Error;
                self.error = Some(msg.clone());
                true
            }
            CoreEvent::Notice(msg) => {
                self.notice = Some(msg.clone());
                true
            }
            // 画面帧与统计由画面组件单独消费，不影响会话状态
            CoreEvent::ScreenFrame(_) | CoreEvent::Stats { .. } | CoreEvent::SignalHole(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_params() -> HostParams {
        HostParams {
            mode: ConnectionMode::Auto,
            relay: "35.209.148.235:8977".into(),
            sid: "pd-AB12C34".into(),
            hole_port: 23517,
            password: "123456".into(),
        }
    }

    fn connect_params() -> ConnectParams {
        ConnectParams {
            mode: ConnectionMode::Auto,
            target: "35.209.148.235:8977".into(),
            sid: "pd-AB12C34".into(),
            password: "123456".into(),
        }
    }

    // ---------- 阶段语义 ----------

    #[test]
    fn 活跃阶段判定与旧实现一致() {
        // 旧代码：connected || authenticated || authentication
        assert!(SessionPhase::Authentication.is_active());
        assert!(SessionPhase::Authenticated.is_active());
        assert!(SessionPhase::Connected.is_active());
        assert!(!SessionPhase::Idle.is_active());
        assert!(!SessionPhase::Disconnected.is_active());
        assert!(!SessionPhase::Error.is_active());
    }

    #[test]
    fn 脉冲动画只在等待阶段() {
        assert!(SessionPhase::Authentication.is_pending());
        assert!(SessionPhase::Authenticated.is_pending());
        assert!(!SessionPhase::Connected.is_pending());
        assert!(!SessionPhase::Idle.is_pending());
    }

    #[test]
    fn 色调映射与旧实现一致() {
        // 旧代码 TONE 表：idle→idle, authentication→connecting, authenticated→online,
        // connected→online, disconnected→idle, error→error
        assert_eq!(SessionPhase::Idle.tone(), StatusTone::Idle);
        assert_eq!(SessionPhase::Authentication.tone(), StatusTone::Connecting);
        assert_eq!(SessionPhase::Authenticated.tone(), StatusTone::Online);
        assert_eq!(SessionPhase::Connected.tone(), StatusTone::Online);
        assert_eq!(SessionPhase::Disconnected.tone(), StatusTone::Idle);
        assert_eq!(SessionPhase::Error.tone(), StatusTone::Error);
    }

    #[test]
    fn 被控端文案与旧实现一致() {
        assert_eq!(SessionPhase::Connected.host_label(), "有人正在观看");
        assert_eq!(SessionPhase::Authentication.host_label(), "等待连接");
        assert_eq!(SessionPhase::Authenticated.host_label(), "等待连接");
        assert_eq!(SessionPhase::Error.host_label(), "出错");
        assert_eq!(SessionPhase::Idle.host_label(), "未开始");
        assert_eq!(SessionPhase::Disconnected.host_label(), "未开始");
    }

    #[test]
    fn 控制端文案与旧实现一致() {
        assert_eq!(SessionPhase::Idle.viewer_label(), "未连接");
        assert_eq!(SessionPhase::Authentication.viewer_label(), "连接中…");
        assert_eq!(SessionPhase::Authenticated.viewer_label(), "已认证，等待画面");
        assert_eq!(SessionPhase::Connected.viewer_label(), "已连接");
        assert_eq!(SessionPhase::Disconnected.viewer_label(), "已断开");
        assert_eq!(SessionPhase::Error.viewer_label(), "出错");
    }

    // ---------- 连接模式 ----------

    #[test]
    fn 连接模式解析与默认值() {
        assert_eq!(ConnectionMode::parse("quic"), ConnectionMode::Quic);
        assert_eq!(ConnectionMode::parse("direct"), ConnectionMode::Direct);
        assert_eq!(ConnectionMode::parse("auto"), ConnectionMode::Auto);
        assert_eq!(ConnectionMode::parse("relay"), ConnectionMode::Relay);
        // 未知值回落到默认
        assert_eq!(ConnectionMode::parse("nonsense"), ConnectionMode::Relay);
        assert_eq!(ConnectionMode::parse(""), ConnectionMode::Relay);
        assert_eq!(ConnectionMode::default(), ConnectionMode::Relay);
    }

    #[test]
    fn 连接模式序列化可被内核识别() {
        for m in [
            ConnectionMode::Relay,
            ConnectionMode::Quic,
            ConnectionMode::Direct,
            ConnectionMode::Auto,
        ] {
            // 往返一致
            assert_eq!(ConnectionMode::parse(m.as_str()), m);
        }
        assert_eq!(ConnectionMode::Relay.as_str(), "relay");
        assert_eq!(ConnectionMode::Auto.as_str(), "auto");
    }

    #[test]
    fn 只有中继与自动模式需要中继地址() {
        // 旧实现：HostPage 里 (mode === 'relay' || mode === 'auto') 才显示中继地址输入框
        assert!(ConnectionMode::Relay.needs_relay_address());
        assert!(ConnectionMode::Auto.needs_relay_address());
        assert!(!ConnectionMode::Quic.needs_relay_address());
        assert!(!ConnectionMode::Direct.needs_relay_address());
    }

    // ---------- 命令校验 ----------

    #[test]
    fn 开关关闭时拒绝起被控端() {
        let mut s = SessionState::default();
        // 默认关（与原实现 AtomicBool::new(false) 一致）
        assert!(!s.allowed);
        let err = s.start_host(host_params()).unwrap_err();
        assert_eq!(err, RejectReason::NotAllowed);
        // 文案与 bridge.rs:108 逐字一致
        assert_eq!(err.message(), "「允许远程控制」开关是关闭的");
        // 被拒绝时不能改变阶段
        assert_eq!(s.phase, SessionPhase::Idle);
    }

    #[test]
    fn 开关打开后允许起被控端() {
        let mut s = SessionState::default();
        s.set_allowed(true);
        let intent = s.start_host(host_params()).unwrap();
        assert_eq!(intent, Intent::StartHost(host_params()));
        assert_eq!(s.phase, SessionPhase::Authentication);
    }

    #[test]
    fn 缺凭据时拒绝且不改阶段() {
        let mut s = SessionState::default();
        s.set_allowed(true);

        let mut p = host_params();
        p.sid = String::new();
        assert_eq!(s.start_host(p).unwrap_err(), RejectReason::MissingCredential);
        assert_eq!(s.phase, SessionPhase::Idle);

        let mut p = host_params();
        p.password = String::new();
        assert_eq!(s.start_host(p).unwrap_err(), RejectReason::MissingCredential);
        assert_eq!(s.phase, SessionPhase::Idle);
    }

    #[test]
    fn 控制端缺凭据时拒绝() {
        let mut s = SessionState::default();
        let mut p = connect_params();
        p.sid = String::new();
        assert_eq!(s.connect(p).unwrap_err(), RejectReason::MissingCredential);
        assert_eq!(s.phase, SessionPhase::Idle);
    }

    #[test]
    fn 控制端不需要允许开关() {
        // 控制端是"我去连别人"，不应受本机「允许远程控制」开关约束
        let mut s = SessionState::default();
        assert!(!s.allowed);
        assert!(s.connect(connect_params()).is_ok());
    }

    #[test]
    fn 活跃会话中不能重复连接() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        assert_eq!(s.phase, SessionPhase::Authentication);

        let err = s.connect(connect_params()).unwrap_err();
        assert_eq!(err, RejectReason::AlreadyRunning);
    }

    #[test]
    fn 开始新会话会清空上一轮残留() {
        let mut s = SessionState::default();
        s.set_allowed(true);
        s.connect(connect_params()).unwrap();
        // 制造一轮残留状态
        s.apply(&CoreEvent::Transport("QUIC 打洞直连".into()));
        s.apply(&CoreEvent::Size { w: 1920, h: 1080 });
        s.apply(&CoreEvent::Notice("打洞端口被占，顺延".into()));
        assert!(s.transport.is_some() && s.screen.is_some() && s.notice.is_some());

        // 断开后重连，所有残留必须清空（对应旧实现里 setState 的 5 个 null）
        s.disconnect();
        s.connect(connect_params()).unwrap();
        assert_eq!(s.phase, SessionPhase::Authentication);
        assert_eq!(s.screen, None);
        assert_eq!(s.error, None);
        assert_eq!(s.notice, None);
        assert_eq!(s.transport, None);
    }

    // ---------- 事件驱动 ----------

    #[test]
    fn 认证成功进入已认证并清除错误() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        s.error = Some("旧的错误".into());

        assert!(s.apply(&CoreEvent::AuthResult { ok: true, reason: None }));
        assert_eq!(s.phase, SessionPhase::Authenticated);
        assert_eq!(s.error, None);
    }

    #[test]
    fn 认证失败进入错误态并使用内核原因() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        s.apply(&CoreEvent::AuthResult {
            ok: false,
            reason: Some("密码错误".into()),
        });
        assert_eq!(s.phase, SessionPhase::Error);
        assert_eq!(s.error.as_deref(), Some("密码错误"));
    }

    #[test]
    fn 认证失败无原因时回落到默认文案() {
        // 旧实现：`e.reason ?? '认证失败'`
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        s.apply(&CoreEvent::AuthResult { ok: false, reason: None });
        assert_eq!(s.error.as_deref(), Some("认证失败"));
    }

    #[test]
    fn 对端连接与断开驱动阶段() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();

        s.apply(&CoreEvent::PeerConnected);
        assert_eq!(s.phase, SessionPhase::Connected);

        s.apply(&CoreEvent::PeerDisconnected);
        assert_eq!(s.phase, SessionPhase::Disconnected);
    }

    #[test]
    fn 分辨率事件填充屏幕尺寸() {
        let mut s = SessionState::default();
        assert_eq!(s.screen, None);
        s.apply(&CoreEvent::Size { w: 1920, h: 1080 });
        assert_eq!(s.screen, Some((1920, 1080)));
    }

    #[test]
    fn 传输路径与提示分别填充() {
        let mut s = SessionState::default();
        s.apply(&CoreEvent::Transport("中继兜底".into()));
        assert_eq!(s.transport.as_deref(), Some("中继兜底"));

        s.apply(&CoreEvent::Notice("打洞端口被占，顺延到 23518".into()));
        assert_eq!(s.notice.as_deref(), Some("打洞端口被占，顺延到 23518"));
        // 提示不应污染传输路径
        assert_eq!(s.transport.as_deref(), Some("中继兜底"));
    }

    #[test]
    fn 错误事件进入错误态() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        s.apply(&CoreEvent::Error("连接被拒绝".into()));
        assert_eq!(s.phase, SessionPhase::Error);
        assert_eq!(s.error.as_deref(), Some("连接被拒绝"));
    }

    #[test]
    fn 画面帧与统计不改变会话状态() {
        // 旧实现里 default 分支 break，画面由 VideoView 单独订阅
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        let before = s.clone();

        assert!(!s.apply(&CoreEvent::ScreenFrame(vec![1, 2, 3])));
        assert!(!s.apply(&CoreEvent::Stats {
            fps: 20,
            kbps: 500,
            ping_ms: 12
        }));
        assert!(!s.apply(&CoreEvent::SignalHole("1.2.3.4:5678".parse().unwrap())));

        // 状态完全没变（帧事件不应触发状态重绘）
        assert_eq!(s, before);
    }

    // ---------- 输入转发 ----------

    #[test]
    fn 仅在已连接时转发输入() {
        let mut s = SessionState::default();
        let msg = InputMsg::MouseMove { x: 10.0, y: 20.0 };

        // 未连接：忽略
        assert_eq!(s.send_input(msg.clone()), Intent::None);

        s.connect(connect_params()).unwrap();
        // 认证中：忽略
        assert_eq!(s.send_input(msg.clone()), Intent::None);

        s.apply(&CoreEvent::PeerConnected);
        // 已连接：转发
        assert_eq!(s.send_input(msg.clone()), Intent::SendInput(msg));
    }

    // ---------- 断开 ----------

    #[test]
    fn 断开返回停止意图并进入已断开() {
        let mut s = SessionState::default();
        s.connect(connect_params()).unwrap();
        s.apply(&CoreEvent::PeerConnected);

        let intent = s.disconnect();
        assert_eq!(intent, Intent::Stop);
        assert_eq!(s.phase, SessionPhase::Disconnected);
    }

    // ---------- 默认值 ----------

    #[test]
    fn 默认状态与旧实现初始值一致() {
        let s = SessionState::default();
        assert_eq!(s.phase, SessionPhase::Idle);
        assert_eq!(s.screen, None);
        assert_eq!(s.error, None);
        assert_eq!(s.notice, None);
        assert_eq!(s.transport, None);
        // 允许开关默认关，必须用户手动打开
        assert!(!s.allowed);
    }

    #[test]
    fn 默认画质与旧实现硬编码一致() {
        assert_eq!(DEFAULT_QUALITY.jpeg, 80);
        assert_eq!(DEFAULT_QUALITY.fps, 20);
    }

    #[test]
    fn 拒绝原因文案齐全() {
        for r in [
            RejectReason::NotAllowed,
            RejectReason::MissingCredential,
            RejectReason::AlreadyRunning,
        ] {
            assert!(!r.message().is_empty(), "{r:?} 缺少文案");
        }
    }
}
