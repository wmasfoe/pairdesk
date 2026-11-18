#!/usr/bin/env bash
# 构建测试所需的全部二进制（含 Linux 专有的 X11 examples）。
#
# 为什么单独一个脚本：
#   tests/ 里的 e2e 脚本需要一个纯色窗口来当"被采集的屏幕"，那是
#   examples/paint（用 x11rb 直接画 X11 窗口）。它只在 Linux 上有意义，
#   因此在 Cargo.toml 里用 `required-features = ["x11-examples"]` 门控，
#   否则 macOS/Windows 上 `cargo check --all-targets` 会失败。
#
#   测试脚本本身不负责构建，所以统一用这个脚本预构建一次。
#
# 用法：
#   ./scripts/build-test-bins.sh          # debug
#   ./scripts/build-test-bins.sh --release
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="debug"
CARGO_FLAGS=()
if [ "${1:-}" = "--release" ]; then
    PROFILE="release"
    CARGO_FLAGS+=("--release")
fi

echo "==> 构建 CLI 与中继"
cargo build "${CARGO_FLAGS[@]}" -p pairdesk-core -p pairdesk-relay

echo "==> 构建 GPUI 壳"
cargo build "${CARGO_FLAGS[@]}" -p pairdesk-ui

if [ "$(uname)" = "Linux" ]; then
    echo "==> 构建 X11 测试辅助（examples/paint, examples/capture_probe）"
    cargo build "${CARGO_FLAGS[@]}" -p pairdesk-core --features x11-examples \
        --example paint --example capture_probe
    echo "    产物: target/${PROFILE}/examples/{paint,capture_probe}"
else
    echo "==> 跳过 X11 examples（非 Linux 平台）"
fi

echo "✅ 构建完成（$PROFILE）"
