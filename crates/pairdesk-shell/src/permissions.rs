//! 权限引导的展示判定。
//!
//! 行为来源：`app/src/components/PermissionBanner.tsx` + `app/src/state/usePermissions.ts`。
//!
//! 原实现把"要不要显示横幅""显示哪一条""按钮该做什么"混在 JSX 里，
//! 这里抽成纯数据，让两套 UI 壳都能复用同一套判定，且能被单测覆盖。

use pairdesk_core::permissions::PermissionStatus;

/// 权限类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    /// 屏幕录制（被控端采集需要）
    Screen,
    /// 辅助功能（控制端注入需要）
    Accessibility,
}

impl PermissionKind {
    /// 传给内核 `request_permission` / `open_permission_settings` 的字符串。
    ///
    /// 与 `permissions.rs` 里的 match 分支一致。
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionKind::Screen => "screen",
            PermissionKind::Accessibility => "accessibility",
        }
    }

    /// 展示名称。
    pub fn label(self) -> &'static str {
        match self {
            PermissionKind::Screen => "屏幕录制",
            PermissionKind::Accessibility => "辅助功能",
        }
    }
}

/// 当前页面需要哪类权限。
///
/// 对应 `PermissionBanner` 的 `requiredFor` prop。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredFor {
    /// 被控端：只需要屏幕录制
    Host,
    /// 控制端：只需要辅助功能
    Viewer,
    /// 首页：两个都要
    Both,
}

impl RequiredFor {
    /// 该角色是否需要某类权限。
    pub fn needs(self, kind: PermissionKind) -> bool {
        match (self, kind) {
            (RequiredFor::Host, PermissionKind::Screen) => true,
            (RequiredFor::Viewer, PermissionKind::Accessibility) => true,
            (RequiredFor::Both, _) => true,
            _ => false,
        }
    }
}

/// 权限引导横幅的渲染指令。
///
/// `None` 表示不显示横幅（权限齐全，或本平台无需引导）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionBanner {
    /// 缺失的权限列表，按展示顺序排列
    pub missing: Vec<PermissionKind>,
}

impl PermissionBanner {
    /// 是否应该显示横幅。
    pub fn is_visible(&self) -> bool {
        !self.missing.is_empty()
    }

    /// 标题文案。
    pub fn title(&self) -> &'static str {
        "需要授予系统权限"
    }
}

/// 根据内核报告的权限状态与当前角色，算出横幅该怎么显示。
///
/// 判定顺序严格对应 `PermissionBanner.tsx`：
/// 1. `needGuidance` 为假（非 macOS）→ 永不显示
/// 2. 按 `requiredFor` 过滤出该角色真正需要的权限
/// 3. 只列出"需要但未授予"的
pub fn banner_for(status: &PermissionStatus, required_for: RequiredFor) -> Option<PermissionBanner> {
    // 非 macOS 平台无需引导，直接返回
    if !status.need_guidance {
        return None;
    }

    let mut missing = Vec::new();

    if required_for.needs(PermissionKind::Screen) && !status.screen_recording {
        missing.push(PermissionKind::Screen);
    }
    if required_for.needs(PermissionKind::Accessibility) && !status.accessibility {
        missing.push(PermissionKind::Accessibility);
    }

    if missing.is_empty() {
        None
    } else {
        Some(PermissionBanner { missing })
    }
}

/// 权限检测失败时的兜底状态。
///
/// 原实现（`usePermissions.ts`）：
/// ```js
/// catch { setStatus({ screenRecording: true, accessibility: true, needGuidance: false }); }
/// ```
/// 即检测失败时**不阻塞用户**，宁可放过也不误弹引导。
pub fn fallback_status() -> PermissionStatus {
    PermissionStatus {
        screen_recording: true,
        accessibility: true,
        need_guidance: false,
    }
}

/// 安全的权限检测：任何 panic 都兜底为"全部已授权"。
///
/// 为什么需要：`pairdesk_core::permissions::check_permissions` 在 macOS 上直接
/// 调 `CGPreflightScreenCaptureAccess` / `AXIsProcessTrusted` 两个 C 函数。
/// 在异常环境（无窗口服务器、被沙箱拦截）下可能 panic，而权限检测失败
/// **绝不能把用户挡在门外**——宁可误判为已授权。
///
/// 抽到这里是为了让 UI 层不必各自写 `catch_unwind`（原先 4 处重复）。
pub fn check_safe() -> PermissionStatus {
    std::panic::catch_unwind(pairdesk_core::permissions::check_permissions)
        .unwrap_or_else(|_| fallback_status())
}

/// 触发系统权限弹窗（如 macOS 屏幕录制授权对话框）。
pub fn request(kind: PermissionKind) -> bool {
    std::panic::catch_unwind(|| pairdesk_core::permissions::request_permission(kind.as_str()))
        .unwrap_or(false)
}

/// 打开系统隐私与安全设置面板。
pub fn open_settings(kind: PermissionKind) {
    // 该函数本身不返回值，panic 也无副作用需要兜底
    let _ = std::panic::catch_unwind(|| {
        pairdesk_core::permissions::open_permission_settings(kind.as_str())
    });
}

/// 权限变化后重启应用。
///
/// macOS 的 TCC 权限结果按进程缓存：`AXIsProcessTrusted` /
/// `CGPreflightScreenCaptureAccess` 一旦在本进程读到 false 就持续返回 false。
/// 用户授权后必须用全新进程重新查询 tccd 才能读到新状态。
pub fn restart_app() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(screen: bool, access: bool, guidance: bool) -> PermissionStatus {
        PermissionStatus {
            screen_recording: screen,
            accessibility: access,
            need_guidance: guidance,
        }
    }

    #[test]
    fn 非_macos_平台永不显示引导() {
        // Linux / Windows 的 check_permissions 返回 need_guidance=false
        let s = status(false, false, false);
        assert_eq!(banner_for(&s, RequiredFor::Both), None);
        assert_eq!(banner_for(&s, RequiredFor::Host), None);
        assert_eq!(banner_for(&s, RequiredFor::Viewer), None);
    }

    #[test]
    fn 权限齐全时不显示引导() {
        let s = status(true, true, true);
        assert_eq!(banner_for(&s, RequiredFor::Both), None);
        assert_eq!(banner_for(&s, RequiredFor::Host), None);
        assert_eq!(banner_for(&s, RequiredFor::Viewer), None);
    }

    #[test]
    fn 被控端只关心屏幕录制权限() {
        // 有屏幕录制、无辅助功能 → 被控端不需要引导
        let s = status(true, false, true);
        assert_eq!(banner_for(&s, RequiredFor::Host), None);

        // 无屏幕录制、有辅助功能 → 被控端需要引导
        let s = status(false, true, true);
        let b = banner_for(&s, RequiredFor::Host).unwrap();
        assert_eq!(b.missing, vec![PermissionKind::Screen]);
        assert!(b.is_visible());
    }

    #[test]
    fn 控制端只关心辅助功能权限() {
        // 有辅助功能、无屏幕录制 → 控制端不需要引导
        let s = status(false, true, true);
        assert_eq!(banner_for(&s, RequiredFor::Viewer), None);

        // 无辅助功能 → 控制端需要引导
        let s = status(true, false, true);
        let b = banner_for(&s, RequiredFor::Viewer).unwrap();
        assert_eq!(b.missing, vec![PermissionKind::Accessibility]);
    }

    #[test]
    fn 首页两类权限都检查且顺序稳定() {
        let s = status(false, false, true);
        let b = banner_for(&s, RequiredFor::Both).unwrap();
        // 顺序必须是屏幕录制在前，与旧实现渲染顺序一致
        assert_eq!(
            b.missing,
            vec![PermissionKind::Screen, PermissionKind::Accessibility]
        );
    }

    #[test]
    fn 部分缺失时只列出缺失项() {
        let s = status(true, false, true);
        let b = banner_for(&s, RequiredFor::Both).unwrap();
        assert_eq!(b.missing, vec![PermissionKind::Accessibility]);

        let s = status(false, true, true);
        let b = banner_for(&s, RequiredFor::Both).unwrap();
        assert_eq!(b.missing, vec![PermissionKind::Screen]);
    }

    #[test]
    fn 角色与权限的对应关系() {
        assert!(RequiredFor::Host.needs(PermissionKind::Screen));
        assert!(!RequiredFor::Host.needs(PermissionKind::Accessibility));

        assert!(!RequiredFor::Viewer.needs(PermissionKind::Screen));
        assert!(RequiredFor::Viewer.needs(PermissionKind::Accessibility));

        assert!(RequiredFor::Both.needs(PermissionKind::Screen));
        assert!(RequiredFor::Both.needs(PermissionKind::Accessibility));
    }

    #[test]
    fn 权限类型字符串可被内核识别() {
        // 必须与 permissions.rs 的 match 分支字面量一致
        assert_eq!(PermissionKind::Screen.as_str(), "screen");
        assert_eq!(PermissionKind::Accessibility.as_str(), "accessibility");
    }

    #[test]
    fn 权限类型有展示名称() {
        assert_eq!(PermissionKind::Screen.label(), "屏幕录制");
        assert_eq!(PermissionKind::Accessibility.label(), "辅助功能");
    }

    #[test]
    fn 检测失败时兜底为不阻塞() {
        // 关键行为：检测失败不能把用户挡在门外
        let s = fallback_status();
        assert!(s.screen_recording);
        assert!(s.accessibility);
        assert!(!s.need_guidance);
        assert_eq!(banner_for(&s, RequiredFor::Both), None);
    }

    #[test]
    fn 安全检测不会_panic() {
        // 无论底层平台 API 如何，check_safe 都必须返回一个可用的状态
        let s = check_safe();
        // 至少字段自洽：need_guidance 为假时不该要求引导
        if !s.need_guidance {
            assert_eq!(banner_for(&s, RequiredFor::Both), None);
        }
    }

    #[test]
    fn 权限操作接受正确类型() {
        // 只验证签名可调用且不 panic（真实系统调用在测试环境无副作用）
        let _ = request(PermissionKind::Screen);
        let _ = request(PermissionKind::Accessibility);
        open_settings(PermissionKind::Screen);
        open_settings(PermissionKind::Accessibility);
    }

    #[test]
    fn 横幅标题与旧实现一致() {
        let s = status(false, false, true);
        let b = banner_for(&s, RequiredFor::Both).unwrap();
        assert_eq!(b.title(), "需要授予系统权限");
    }
}
