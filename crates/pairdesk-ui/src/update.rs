//! 更新检查的网络部分。
//!
//! 纯计算（版本比较、资产匹配）在 `pairdesk-shell::update`，有单测覆盖。
//! 这里只负责发起 HTTP 请求并把结果交给领域层组装。

use pairdesk_shell::update::{build_update_info, UpdateInfo};

const RELEASES_API: &str = "https://api.github.com/repos/wmasfoe/pairdesk/releases/latest";

/// 当前版本。编译期注入，与 Cargo.toml 一致。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 请求 GitHub 最新 release 并组装更新信息。
pub async fn check() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("PairDesk-App")
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(RELEASES_API)
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("GitHub API 返回状态码: {}", res.status()));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("解析更新响应失败: {e}"))?;

    let tag_name = json["tag_name"].as_str().unwrap_or("");
    let release_notes = json["body"].as_str().unwrap_or("");
    let fallback_url = json["html_url"].as_str().unwrap_or("");

    // 资产列表：(name, browser_download_url)
    let assets: Vec<(String, Option<String>)> = json["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| {
                    (
                        a["name"].as_str().unwrap_or("").to_string(),
                        a["browser_download_url"].as_str().map(str::to_string),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(build_update_info(
        current_version(),
        tag_name,
        release_notes,
        assets,
        fallback_url,
    ))
}

// ---------- 应用内下载与安装 ----------

/// 下载进度。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percent: u32,
}

/// 下载安装包到临时目录，返回文件路径。
///
/// `on_progress` 会被频繁调用，调用方应避免在其中做重活。
pub async fn download_installer(
    url: &str,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<std::path::PathBuf, String> {
    use futures_util::StreamExt;
    use std::io::Write;

    if url.is_empty() {
        return Err("下载地址为空".into());
    }

    let client = reqwest::Client::builder()
        .user_agent("PairDesk-App")
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求安装包失败: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("下载失败，状态码: {}", res.status()));
    }

    let total = res.content_length().unwrap_or(0);
    let ext = installer_extension();
    let path = std::env::temp_dir().join(format!("pairdesk-update-latest.{ext}"));

    let mut file = std::fs::File::create(&path).map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取下载数据流失败: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("写入安装包失败: {e}"))?;
        downloaded += chunk.len() as u64;

        let percent = if total > 0 {
            (downloaded as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };
        on_progress(DownloadProgress {
            downloaded,
            total,
            percent,
        });
    }
    drop(file);

    Ok(path)
}

/// 当前平台的安装包扩展名。与 `pairdesk_shell::update` 的资产匹配规则一致。
fn installer_extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "dmg"
    } else if cfg!(target_os = "windows") {
        "msi"
    } else {
        "AppImage"
    }
}

/// 执行平台特定的覆盖安装，然后退出当前进程。
///
/// 与原 `bridge.rs::pd_install_update` 的行为一致：
/// - macOS：挂载 dmg → 覆盖 /Applications/PairDesk.app → 去 quarantine → 重启
/// - Windows：msiexec 静默安装 → 退出
/// - Linux：赋可执行权限 → 启动新版本 → 退出
pub fn install_and_exit(installer: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"
            MOUNT=$(hdiutil attach -nobrowse -readonly "{dmg}" | tail -1 | awk '{{print $NF}}')
            if [ -n "$MOUNT" ]; then
                rm -rf "/Applications/PairDesk.app"
                cp -R "$MOUNT/PairDesk.app" /Applications/
                codesign --force --deep --sign - "/Applications/PairDesk.app" 2>/dev/null || true
                xattr -dr com.apple.quarantine "/Applications/PairDesk.app" 2>/dev/null || true
                hdiutil detach "$MOUNT" -quiet 2>/dev/null || true
                rm -f "{dmg}"
                open "/Applications/PairDesk.app"
            fi
            "#,
            dmg = installer.display()
        );
        std::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .spawn()
            .map_err(|e| format!("启动 macOS 更新脚本失败: {e}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("msiexec")
            .args(["/i", &installer.display().to_string(), "/quiet", "/norestart"])
            .spawn()
            .map_err(|e| format!("启动 Windows MSI 安装失败: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(mut perms) = std::fs::metadata(installer).map(|m| m.permissions()) {
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(installer, perms);
        }
        std::process::Command::new(installer)
            .spawn()
            .map_err(|e| format!("启动新版 Linux 客户端失败: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 安装包扩展名与平台匹配() {
        let ext = installer_extension();
        if cfg!(target_os = "macos") {
            assert_eq!(ext, "dmg");
        } else if cfg!(target_os = "windows") {
            assert_eq!(ext, "msi");
        } else {
            assert_eq!(ext, "AppImage");
        }
    }

    #[test]
    fn 当前版本非空() {
        assert!(!current_version().is_empty());
    }

    #[tokio::test]
    async fn 空地址拒绝下载() {
        // 不应发起网络请求就报错
        let r = download_installer("", |_| {}).await;
        assert!(r.is_err());
    }
}
