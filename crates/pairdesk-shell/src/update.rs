//! 更新检查：版本比较与平台资产匹配。
//!
//! 行为来源：`crates/pairdesk-app/src/bridge.rs` 的 `pd_check_update`。
//!
//! 原实现把「请求 GitHub API」「解析 JSON」「按平台挑资产」全塞在一个
//! `#[tauri::command]` 里。这里把**纯计算部分**（版本比较、资产匹配）抽出来，
//! 网络请求留给适配器层。这样版本逻辑可以被单测穷举，而不必真的打网络。

/// 更新信息。字段与前端 `UpdateInfo` 一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub download_url: String,
    /// 安装包的 sha256（GitHub Release 的 `asset.digest`，形如 `sha256:abcd...`）。
    /// 下载后用于完整性校验；缺失时跳过校验（旧 release 可能没有该字段）。
    pub digest: Option<String>,
}

/// 目标平台三元组，用于决定匹配哪种安装包。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Aarch64,
    X86_64,
    Other,
}

impl Platform {
    /// 当前运行平台。
    pub fn current() -> Self {
        let os = if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        };
        let arch = if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else {
            Arch::Other
        };
        Self { os, arch }
    }
}

/// 决定用哪个文件名片段去匹配 GitHub Release 资产。
///
/// 原实现（`bridge.rs::pd_check_update` 里的 `target_pattern` 分支）：
/// ```text
/// if cfg!(target_os = "macos") {
///     if cfg!(target_arch = "aarch64") { "aarch64.dmg" } else { "x86_64.dmg" }
/// } else if cfg!(target_os = "windows") { ".msi" } else { ".AppImage" }
/// ```
pub fn platform_asset_pattern(platform: Platform) -> &'static str {
    match platform.os {
        Os::MacOs => match platform.arch {
            Arch::Aarch64 => "aarch64.dmg",
            // 非 aarch64 一律当 x86_64，与旧实现的 else 分支一致
            _ => "x86_64.dmg",
        },
        Os::Windows => ".msi",
        Os::Linux => ".AppImage",
    }
}

/// 从 GitHub Release 的 `tag_name` 中提取纯版本号。
///
/// 原实现：`.trim_start_matches('v')`
pub fn normalize_version(tag: &str) -> String {
    tag.trim_start_matches('v').to_string()
}

/// 判断是否需要更新。
///
/// 原实现：`!tag_name.is_empty() && tag_name != current_version`
///
/// 注意：这是**朴素字符串比较**，不是语义化版本比较。`0.1.9` 与 `0.1.10`
/// 会被判为不同（正确），但 `0.1.21` 与 `0.1.21` 相同（正确）。
/// 保留原行为以保证迁移后行为一致；语义化比较是后续独立改进项。
pub fn has_update(current: &str, latest_tag: &str) -> bool {
    let latest = normalize_version(latest_tag);
    !latest.is_empty() && latest != current
}

/// 从资产列表里挑出匹配当前平台下载地址。
///
/// 对应原实现的 `json["assets"].as_array().find_map(...)`：
/// 第一个 `name` 包含目标片段且带 `browser_download_url` 的资产。
///
/// `assets` 是 `(name, browser_download_url)` 序列。
pub fn pick_asset<'a, I>(assets: I, pattern: &str) -> Option<String>
where
    I: IntoIterator<Item = (&'a str, Option<&'a str>)>,
{
    assets
        .into_iter()
        .find(|(name, url)| name.contains(pattern) && url.is_some())
        .and_then(|(_, url)| url.map(str::to_string))
}

/// 在资产列表里找出与 `url` 对应的 digest（sha256）。
///
/// 单独一个函数是因为 `build_update_info` 的资产类型保持 `(name, url)` 不变，
/// 避免改动已有调用点与单测；digest 由适配器层在拿到 JSON 后补进来。
pub fn digest_for<'a, I>(assets: I, url: &str) -> Option<String>
where
    I: IntoIterator<Item = (&'a str, Option<&'a str>, Option<&'a str>)>,
{
    assets
        .into_iter()
        .find(|(_, u, _)| u.map(|x| x == url).unwrap_or(false))
        .and_then(|(_, _, d)| d.map(str::to_string))
}

/// 组装最终的更新信息。
///
/// 参数是已经从 GitHub JSON 里取出的字段；网络与解析由适配器负责。
pub fn build_update_info(
    current_version: &str,
    tag_name: &str,
    release_notes: &str,
    assets: Vec<(String, Option<String>)>,
    fallback_url: &str,
    digest: Option<String>,
) -> UpdateInfo {
    let latest = normalize_version(tag_name);
    let pattern = platform_asset_pattern(Platform::current());

    let borrowed: Vec<(&str, Option<&str>)> = assets
        .iter()
        .map(|(n, u)| (n.as_str(), u.as_deref()))
        .collect();

    let download_url = pick_asset(borrowed, pattern).unwrap_or_else(|| fallback_url.to_string());

    UpdateInfo {
        current_version: current_version.to_string(),
        latest_version: latest.clone(),
        has_update: !latest.is_empty() && latest != current_version,
        release_notes: release_notes.to_string(),
        download_url,
        digest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 版本归一化 ----------

    #[test]
    fn 版本号去掉_v_前缀() {
        assert_eq!(normalize_version("v0.1.21"), "0.1.21");
        assert_eq!(normalize_version("0.1.21"), "0.1.21");
        // trim_start_matches 会去掉所有连续的 v
        assert_eq!(normalize_version("vv0.1.21"), "0.1.21");
        assert_eq!(normalize_version(""), "");
        // 中间的 v 不受影响
        assert_eq!(normalize_version("v1.0.0-dev"), "1.0.0-dev");
    }

    // ---------- 更新判定 ----------

    #[test]
    fn 版本相同时无需更新() {
        assert!(!has_update("0.1.21", "v0.1.21"));
        assert!(!has_update("0.1.21", "0.1.21"));
    }

    #[test]
    fn 版本不同时需要更新() {
        assert!(has_update("0.1.20", "v0.1.21"));
        assert!(has_update("0.1.21", "v0.1.22"));
    }

    #[test]
    fn 空_tag_视为无需更新() {
        // 原实现：`!tag_name.is_empty() && ...`，防止 GitHub 返回空值时误报
        assert!(!has_update("0.1.21", ""));
        assert!(!has_update("0.1.21", "v"));
    }

    #[test]
    fn 朴素比较保留原行为_含降级误报() {
        // 这是原实现的已知局限：GitHub 上最新版比本地旧时也会报"有更新"。
        // 迁移必须保持行为一致，语义化比较是后续独立改进项。
        assert!(has_update("0.1.21", "v0.1.20"));
        // 字符串比较：0.1.9 != 0.1.10，虽然语义上 0.1.10 更新
        assert!(has_update("0.1.9", "v0.1.10"));
    }

    // ---------- 平台资产匹配 ----------

    #[test]
    fn macos_按架构匹配_dmg() {
        let arm = Platform {
            os: Os::MacOs,
            arch: Arch::Aarch64,
        };
        assert_eq!(platform_asset_pattern(arm), "aarch64.dmg");

        let intel = Platform {
            os: Os::MacOs,
            arch: Arch::X86_64,
        };
        assert_eq!(platform_asset_pattern(intel), "x86_64.dmg");
    }

    #[test]
    fn macos_未知架构回落到_intel() {
        // 原实现是 if aarch64 { ... } else { x86_64.dmg }
        let other = Platform {
            os: Os::MacOs,
            arch: Arch::Other,
        };
        assert_eq!(platform_asset_pattern(other), "x86_64.dmg");
    }

    #[test]
    fn windows_与_linux_匹配正确() {
        assert_eq!(
            platform_asset_pattern(Platform {
                os: Os::Windows,
                arch: Arch::X86_64
            }),
            ".msi"
        );
        assert_eq!(
            platform_asset_pattern(Platform {
                os: Os::Linux,
                arch: Arch::X86_64
            }),
            ".AppImage"
        );
    }

    #[test]
    fn 当前平台能被识别() {
        let p = Platform::current();
        // 至少应落到某个已知 OS 上，且 pattern 非空
        assert!(!platform_asset_pattern(p).is_empty());
    }

    // ---------- 资产挑选 ----------

    #[test]
    fn 挑选第一个匹配的资产() {
        let assets = vec![
            ("PairDesk_0.1.22_x64.msi", Some("https://example.com/a.msi")),
            ("PairDesk_0.1.22_aarch64.dmg", Some("https://example.com/b.dmg")),
            ("PairDesk_0.1.22_amd64.AppImage", Some("https://example.com/c.AppImage")),
        ];
        assert_eq!(
            pick_asset(assets.clone(), ".msi"),
            Some("https://example.com/a.msi".to_string())
        );
        assert_eq!(
            pick_asset(assets.clone(), "aarch64.dmg"),
            Some("https://example.com/b.dmg".to_string())
        );
        assert_eq!(
            pick_asset(assets, ".AppImage"),
            Some("https://example.com/c.AppImage".to_string())
        );
    }

    #[test]
    fn 跳过没有下载地址的资产() {
        // 关键：名称匹配但 url 为 null 时必须继续找下一个
        let assets = vec![
            ("PairDesk.msi", None),
            ("PairDesk_other.msi", Some("https://example.com/ok.msi")),
        ];
        assert_eq!(
            pick_asset(assets, ".msi"),
            Some("https://example.com/ok.msi".to_string())
        );
    }

    #[test]
    fn 无匹配资产时返回_none() {
        let assets = vec![("PairDesk.dmg", Some("https://example.com/a.dmg"))];
        assert_eq!(pick_asset(assets, ".msi"), None);
        assert_eq!(pick_asset(Vec::<(&str, Option<&str>)>::new(), ".msi"), None);
    }

    // ---------- 组装 ----------

    #[test]
    fn 组装更新信息_有匹配资产() {
        let pattern = platform_asset_pattern(Platform::current());
        let asset_name = format!("PairDesk_0.1.22_{pattern}");

        let info = build_update_info(
            "0.1.21",
            "v0.1.22",
            "修复若干问题",
            vec![(asset_name, Some("https://example.com/pkg".into()))],
            "https://github.com/wmasfoe/pairdesk/releases",
            None,
        );

        assert_eq!(info.current_version, "0.1.21");
        assert_eq!(info.latest_version, "0.1.22");
        assert!(info.has_update);
        assert_eq!(info.release_notes, "修复若干问题");
        assert_eq!(info.download_url, "https://example.com/pkg");
    }

    #[test]
    fn 组装更新信息_无匹配资产时回落到发布页() {
        // 原实现：找不到匹配资产时用 `json["html_url"]`
        let info = build_update_info(
            "0.1.21",
            "v0.1.22",
            "",
            vec![("不相关的文件.zip".into(), Some("https://example.com/x".into()))],
            "https://github.com/wmasfoe/pairdesk/releases/tag/v0.1.22",
            None,
        );
        assert_eq!(
            info.download_url,
            "https://github.com/wmasfoe/pairdesk/releases/tag/v0.1.22"
        );
    }

    #[test]
    fn 组装更新信息_已是最新() {
        let info = build_update_info("0.1.21", "v0.1.21", "", vec![], "https://example.com", None);
        assert!(!info.has_update);
        assert_eq!(info.latest_version, "0.1.21");
    }
}
