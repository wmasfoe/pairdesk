//! 守护测试：防止在 GPUI 的异步执行器里使用 tokio 生态的异步 API。
//!
//! # 为什么需要这个测试
//!
//! GPUI 的 executor 基于 `async-task` + `smol`，不是 tokio。在 `cx.spawn`
//! 的 future 里 `.await` tokio-only 的 API（如 `reqwest` 的异步版、`hyper`、
//! `tokio::net`）会 panic：
//!
//! ```text
//! there is no reactor running, must be called from the context of a Tokio 1.x runtime
//! ```
//!
//! panic 发生在 executor 线程里 → **整个进程退出**，用户看到的是「点一下 App 就没了」。
//!
//! 这个 bug 真实发生过（点「检查更新」直接退出），排查成本很高：应用层没有任何
//! 错误日志，只有系统日志里一行 hyper 的 panic。
//!
//! 因此用静态扫描把这类写法挡在 CI 里。
//! 详见 `docs/architecture/gpui-constraints.md` 第 3 节。

use std::path::Path;

/// 禁止出现在源码里的模式：(模式, 原因)。
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "reqwest::get(",
        "reqwest 异步 API 依赖 tokio reactor，请用 reqwest::blocking + 独立线程",
    ),
    (
        "reqwest::Client::new()",
        "reqwest 异步 Client 依赖 tokio，请用 reqwest::blocking::Client",
    ),
    ("hyper::", "hyper 依赖 tokio reactor"),
    ("tokio::net::", "tokio::net 依赖 tokio reactor"),
    ("tokio::fs::", "tokio::fs 依赖 tokio reactor"),
    (
        "tokio::time::sleep",
        "请用 cx.background_executor().timer(..)",
    ),
    ("tokio::spawn(", "请用 cx.spawn(..) 或 std::thread::spawn"),
];

/// 检查单个文件，返回违规列表。
fn scan_file(path: &Path) -> Vec<String> {
    let Ok(src) = std::fs::read_to_string(path) else {
        return vec![];
    };

    // 扫描器自身的 FORBIDDEN 表里就有这些字符串，跳过它避免自伤
    if path
        .file_name()
        .map(|n| n == "no_tokio_in_gpui_executor.rs")
        .unwrap_or(false)
    {
        return vec![];
    }

    let mut violations = vec![];
    for (lineno, line) in src.lines().enumerate() {
        let trimmed = line.trim_start();

        // 跳过注释行（文档里会引用这些 API 名做说明）
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }

        for (pattern, reason) in FORBIDDEN {
            if line.contains(pattern) {
                // 例外：`reqwest::blocking::Client::new()` 是允许的
                if line.contains("reqwest::blocking") {
                    continue;
                }
                violations.push(format!(
                    "{}:{}: 出现 `{}` —— {}",
                    path.display(),
                    lineno + 1,
                    pattern,
                    reason
                ));
            }
        }
    }
    violations
}

/// 递归扫描目录下的 .rs 文件。
fn scan_dir(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            scan_dir(&p, out);
        } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
            out.extend(scan_file(&p));
        }
    }
}

#[test]
fn gpui_壳里不得使用_tokio_生态的异步_api() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = vec![];
    scan_dir(&src, &mut violations);

    assert!(
        violations.is_empty(),
        "\n发现 {} 处违反 GPUI 执行器约束的写法：\n\n{}\n\n\
         原因：GPUI 的 executor 是 async-task + smol，不是 tokio。\n\
         在上面这些 API 上 .await 会 panic（no reactor running），\n\
         且 panic 发生在 executor 线程 → 整个进程退出。\n\n\
         正确做法：改用阻塞版 API + 独立 std 线程，结果经 channel 回传。\n\
         参考 docs/architecture/gpui-constraints.md 第 3 节。\n",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn 扫描器本身能识别违规写法() {
    // 自检：确保扫描逻辑真的会报错，而不是永远通过
    let tmp = std::env::temp_dir().join("pd-tokio-scan-selftest.rs");
    std::fs::write(&tmp, "fn bad() {\n    let _ = reqwest::get(\"http://x\");\n}\n").unwrap();
    assert_eq!(scan_file(&tmp).len(), 1, "应识别出 reqwest::get");
    let _ = std::fs::remove_file(&tmp);

    // 注释里的引用不算违规
    let tmp2 = std::env::temp_dir().join("pd-tokio-scan-ok.rs");
    std::fs::write(&tmp2, "// 不要用 reqwest::get(\nfn ok() {}\n").unwrap();
    assert!(scan_file(&tmp2).is_empty(), "注释行不应报错");
    let _ = std::fs::remove_file(&tmp2);

    // blocking 版是允许的
    let tmp3 = std::env::temp_dir().join("pd-tokio-scan-blocking.rs");
    std::fs::write(
        &tmp3,
        "fn ok() {\n    let c = reqwest::blocking::Client::new();\n}\n",
    )
    .unwrap();
    assert!(scan_file(&tmp3).is_empty(), "blocking 版应允许");
    let _ = std::fs::remove_file(&tmp3);
}
