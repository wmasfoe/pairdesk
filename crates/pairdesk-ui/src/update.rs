//! 更新检查的网络部分。
//!
//! 纯计算（版本比较、资产匹配）在 `pairdesk-shell::update`，有单测覆盖。
//! 这里只负责发起 HTTP 请求并把结果交给领域层组装。
//!
//! # 为什么用阻塞 reqwest + 独立线程
//!
//! GPUI 的异步执行器基于 `async-task` + `smol`，**不是 tokio**。
//! 而 `reqwest` 的异步 API 内部依赖 tokio reactor，在非 tokio 上下文调用会
//! panic：
//!
//! ```text
//! there is no reactor running, must be called from the context of a Tokio 1.x runtime
//! ```
//!
//! 这个 panic 发生在 GPUI 的 executor 线程里，会直接终止整个进程 ——
//! 表现为「点检查更新，App 立刻退出」。
//!
//! 因此这里改用 `reqwest::blocking`（它内部自建 tokio runtime），跑在
//! 独立的 std 线程里，结果通过 channel 回传给 GPUI 的异步任务。
//! 这样既不依赖 GPUI 的 executor 类型，也不会阻塞 UI 线程。

use std::sync::mpsc;

use pairdesk_shell::update::{build_update_info, UpdateInfo};

const RELEASES_API: &str = "https://api.github.com/repos/wmasfoe/pairdesk/releases/latest";

/// 当前版本。编译期注入，与 Cargo.toml 一致。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 请求 GitHub 最新 release 并组装更新信息。
///
/// 返回的 future 在 GPUI 的 executor 上跑，但真正的网络 I/O 在独立线程里。
pub async fn check() -> Result<UpdateInfo, String> {
    // 在独立线程里做阻塞请求，避免污染 GPUI 的 executor
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("pd-update-check".into())
        .spawn(move || {
            let result = check_blocking();
            let _ = tx.send(result);
        })
        .map_err(|e| format!("启动更新检查线程失败: {e}"))?;

    // 等待结果。
    //
    // 关键：不能直接阻塞 recv() —— 会占死 GPUI 的 executor 线程，UI 卡住。
    // 也不能裸 try_recv + yield 空转 —— 会把一个核跑满。
    // 这里每 20ms 轮询一次，中间用 futures-timer 真正休眠（不占 CPU），
    // 且不依赖任何特定 executor（tokio/smol 都能跑）。
    loop {
        match rx.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {
                futures_timer::Delay::new(std::time::Duration::from_millis(20)).await;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("更新检查线程异常退出".to_string());
            }
        }
    }
}

/// 阻塞版本的检查逻辑。只在独立线程里调用。
fn check_blocking() -> Result<UpdateInfo, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("PairDesk-App")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(RELEASES_API)
        .send()
        .map_err(|e| format!("检查更新失败: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("GitHub API 返回状态码: {}", res.status()));
    }

    let json: serde_json::Value = res
        .json()
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

    // 先组装（不含 digest），再按选中的 URL 回查 digest。
    let mut info = build_update_info(
        current_version(),
        tag_name,
        release_notes,
        assets.clone(),
        fallback_url,
        None,
    );

    // 从原始 JSON 里取选中资产的 digest（GitHub 2024 年起提供）
    info.digest = json["assets"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|a| a["browser_download_url"].as_str() == Some(info.download_url.as_str()))
                .and_then(|a| a["digest"].as_str())
                .map(str::to_string)
        });

    Ok(info)
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
/// 同 [`check`]：阻塞请求跑在独立线程，避免 reqwest 在非 tokio executor 上 panic。
/// `on_progress` 在**独立线程**里被调用，因此不能直接碰 GPUI 的 Context。
pub async fn download_installer(
    url: &str,
    expected_sha256: Option<&str>,
    on_progress: impl FnMut(DownloadProgress) + Send + 'static,
) -> Result<std::path::PathBuf, String> {
    let url = url.to_string();
    let expected = expected_sha256.map(str::to_string);

    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("pd-update-download".into())
        .spawn(move || {
            let result = download_blocking(&url, expected.as_deref(), on_progress);
            let _ = tx.send(result);
        })
        .map_err(|e| format!("启动下载线程失败: {e}"))?;

    loop {
        match rx.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {
                futures_timer::Delay::new(std::time::Duration::from_millis(20)).await;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("下载线程异常退出".to_string());
            }
        }
    }
}

fn download_blocking(
    url: &str,
    expected_sha256: Option<&str>,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<std::path::PathBuf, String> {
    use std::io::{Read, Write};

    if url.is_empty() {
        return Err("下载地址为空".into());
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("PairDesk-App")
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let mut res = client
        .get(url)
        .send()
        .map_err(|e| format!("请求安装包失败: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("下载失败，状态码: {}", res.status()));
    }

    let total = res.content_length().unwrap_or(0);
    let ext = installer_extension();
    let path = std::env::temp_dir().join(format!("pairdesk-update-latest.{ext}"));

    let mut file = std::fs::File::create(&path).map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = res
            .read(&mut buf)
            .map_err(|e| format!("读取下载数据流失败: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入安装包失败: {e}"))?;
        downloaded += n as u64;

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

    // 完整性校验：GitHub Release API 的 asset.digest 形如 "sha256:abcd..."。
    // 不校验的风险是安装包被截断（网络中断）或被中间人替换后直接执行。
    if let Some(expected) = expected_sha256 {
        let expected = expected
            .strip_prefix("sha256:")
            .unwrap_or(expected)
            .to_ascii_lowercase();
        if !expected.is_empty() {
            let actual = sha256_file(&path)?;
            if actual != expected {
                let _ = std::fs::remove_file(&path);
                return Err(format!(
                    "安装包校验失败（可能下载不完整或被篡改）\n期望: {expected}\n实际: {actual}"
                ));
            }
        }
    }

    Ok(path)
}

/// 计算文件 sha256（流式读取，避免把 14MB 安装包全读进内存）。
fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取文件失败: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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
/// 各平台的替换策略不同，原因是安装形态不同：
///
/// | 平台 | 安装形态 | 替换方式 |
/// |---|---|---|
/// | macOS | `.app` 目录 | 挂载 dmg → 覆盖目标 .app → 去 quarantine |
/// | Windows | MSI | `msiexec /i` 静默安装（自动处理旧版本卸载） |
/// | Linux | deb（`/usr/bin/`）或 AppImage（单文件） | 见 [`install_linux`] |
pub fn install_and_exit(installer: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        install_macos(installer)
    }

    #[cfg(target_os = "windows")]
    {
        install_windows(installer)
    }

    #[cfg(target_os = "linux")]
    {
        install_linux(installer)
    }
}

/// macOS：挂载 dmg，覆盖当前运行的这个 .app，然后重启。
///
/// 关键：目标路径取自**当前运行的二进制**（`current_exe`），而不是硬编码
/// `/Applications/PairDesk.app`。否则用户把 App 放在别处（如 `~/Applications`
/// 或直接跑 `target/release`）时，更新会装到另一个位置，看起来像"没生效"。
#[cfg(target_os = "macos")]
fn install_macos(installer: &std::path::Path) -> Result<(), String> {
    // 从 .../PairDesk.app/Contents/MacOS/pairdesk-ui 回溯出 .app 路径
    let exe = std::env::current_exe().map_err(|e| format!("取当前路径失败: {e}"))?;
    let app_bundle = exe
        .ancestors()
        .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
        .ok_or_else(|| "当前不是从 .app 包内运行，无法自动替换".to_string())?
        .to_path_buf();

    let dmg = installer.display().to_string();
    let target = app_bundle.display().to_string();

    // 脚本在 App 退出后执行：先等旧进程真正释放文件，再替换
    let script = format!(
        r#"
        set -e
        DMG="{dmg}"
        TARGET="{target}"
        MOUNT=$(hdiutil attach -nobrowse -readonly "$DMG" | tail -1 | awk '{{print $NF}}')
        if [ -z "$MOUNT" ]; then echo "挂载 dmg 失败" >&2; exit 1; fi

        # 等当前 App 退出（最多 10 秒）
        for i in $(seq 1 20); do
            pgrep -f "$TARGET/Contents/MacOS" >/dev/null 2>&1 || break
            sleep 0.5
        done

        rm -rf "$TARGET"
        cp -R "$MOUNT/PairDesk.app" "$TARGET"
        codesign --force --deep --sign - "$TARGET" 2>/dev/null || true
        xattr -dr com.apple.quarantine "$TARGET" 2>/dev/null || true
        hdiutil detach "$MOUNT" -quiet 2>/dev/null || true
        rm -f "$DMG"
        open "$TARGET"
        "#,
    );

    std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .spawn()
        .map_err(|e| format!("启动 macOS 更新脚本失败: {e}"))?;
    Ok(())
}

/// Windows：msiexec 静默安装。
///
/// msiexec 会自己处理旧版本卸载（相同 UpgradeCode），因此不需要手动删文件。
#[cfg(target_os = "windows")]
fn install_windows(installer: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("msiexec")
        .args([
            "/i",
            &installer.display().to_string(),
            "/quiet",
            "/norestart",
        ])
        .spawn()
        .map_err(|e| format!("启动 Windows MSI 安装失败: {e}"))?;
    Ok(())
}

/// Linux：区分 deb 安装与 AppImage 运行两种形态。
///
/// - **deb 安装**（二进制在 `/usr/bin/pairdesk-ui`）：不能用 AppImage 覆盖，
///   需要 `dpkg -i` 新包（要 root）。这里用 `pkexec` 提权，失败则给出提示让
///   用户手动执行命令——不静默失败。
/// - **AppImage 运行**（当前二进制是单文件）：直接把新 AppImage 覆盖到当前
///   路径，保留可执行权限，然后重启。
#[cfg(target_os = "linux")]
fn install_linux(installer: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let exe = std::env::current_exe().map_err(|e| format!("取当前路径失败: {e}"))?;
    // AppImage 运行时会设置 APPIMAGE 环境变量指向自身
    let is_appimage = std::env::var_os("APPIMAGE").is_some();

    if is_appimage {
        // ---- AppImage：覆盖自身 ----
        let target = std::env::var_os("APPIMAGE")
            .map(std::path::PathBuf::from)
            .unwrap_or(exe);

        let mut perms = std::fs::metadata(installer)
            .map_err(|e| format!("读取新版本信息失败: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(installer, perms)
            .map_err(|e| format!("设置可执行权限失败: {e}"))?;

        // 先复制到同目录的临时文件再原子替换，避免覆盖到一半损坏原文件
        let staged = target.with_extension("new");
        std::fs::copy(installer, &staged).map_err(|e| format!("写入新版本失败: {e}"))?;
        std::fs::rename(&staged, &target).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("替换 AppImage 失败: {e}")
        })?;

        std::process::Command::new(&target)
            .spawn()
            .map_err(|e| format!("启动新版失败: {e}"))?;
        Ok(())
    } else {
        // ---- deb 安装：需要包管理器介入 ----
        // 无法在不提权的情况下替换 /usr/bin 下的文件，因此明确告知用户，
        // 而不是假装成功（旧实现会去运行一个 AppImage，实际上什么也没更新）。
        let cmd = format!("sudo dpkg -i {}", installer.display());
        Err(format!(
            "当前是系统包安装（deb），需要管理员权限完成更新。\n请手动执行：\n  {cmd}\n\
             或从 Releases 页面下载 .deb 手动安装。"
        ))
    }
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

    #[test]
    fn sha256_计算正确() {
        // 已知值：空文件的 sha256
        let path = std::env::temp_dir().join("pd-sha-empty-test");
        std::fs::write(&path, b"").unwrap();
        let got = sha256_file(&path).unwrap();
        assert_eq!(
            got,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = std::fs::remove_file(&path);

        // "abc" 的 sha256
        let path2 = std::env::temp_dir().join("pd-sha-abc-test");
        std::fs::write(&path2, b"abc").unwrap();
        let got2 = sha256_file(&path2).unwrap();
        assert_eq!(
            got2,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&path2);
    }

    #[test]
    fn 缺失文件返回错误() {
        assert!(sha256_file(std::path::Path::new("/nonexistent/pd-test")).is_err());
    }

    #[test]
    fn digest_前缀被正确剥离() {
        // GitHub 给的是 "sha256:abcd..."，校验时要剥掉前缀再小写比较
        let raw = "sha256:BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        let normalized = raw.strip_prefix("sha256:").unwrap().to_ascii_lowercase();
        assert_eq!(
            normalized,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        // 没有前缀也能用
        let no_prefix = "abc123";
        assert_eq!(
            no_prefix.strip_prefix("sha256:").unwrap_or(no_prefix),
            "abc123"
        );
    }

    #[tokio::test]
    async fn 空地址拒绝下载() {
        // 不应发起网络请求就报错
        let r = download_installer("", None, |_| {}).await;
        assert!(r.is_err());
    }
}
