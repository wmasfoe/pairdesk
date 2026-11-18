#!/usr/bin/env bash
# ============================================================
# PairDesk Linux 一键安装
#
# 默认装 .deb（Debian/Ubuntu 系，需要 sudo）；
# 非 deb 系或想免 root，用 --appimage（装到 ~/.local/bin）。
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-linux.sh | sudo bash
#   curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-linux.sh | bash -s -- --appimage
# ============================================================
set -euo pipefail

REPO="wmasfoe/pairdesk"
MODE="deb"
[ "${1:-}" = "--appimage" ] && MODE="appimage"

# 注意：deb 与 AppImage 的架构命名不同——
#   .deb      用 Debian 命名：amd64 / arm64
#   .AppImage 用 Rust target 命名：x86_64 / aarch64
# 用同一个变量会导致其中一种永远匹配失败。
case "$(uname -m)" in
  x86_64|amd64)  ARCH_DEB="amd64"; ARCH_APPIMAGE="x86_64" ;;
  aarch64|arm64) ARCH_DEB="arm64"; ARCH_APPIMAGE="aarch64" ;;
  *) echo "❌ 不支持的架构: $(uname -m)"; exit 1 ;;
esac

# ---------- 解析最新 release 资产 URL ----------
get_url() { # $1 = 匹配模式
  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -oE "https://[^\"]*$1\"" | tr -d '"' | head -1
}

if [ "$MODE" = "deb" ]; then
  # 检测是否 deb 系
  if [ -f /etc/os-release ]; then
    . /etc/os-release
  fi
  case "${ID:-}" in
    debian|ubuntu) : ;;
    *) echo "⚠️ 当前系统不是 Debian/Ubuntu 系，建议用 --appimage 免 root 安装"; exit 1 ;;
  esac
  if [ "$(id -u)" -ne 0 ]; then
    echo "❌ .deb 安装需要 root（用: sudo bash 或加 --appimage）"; exit 1
  fi

  echo "==> [1/3] 解析 .deb 下载地址"
  # 文件名前缀两种都可能出现：cargo-packager 用二进制名（pairdesk-ui_），
  # 旧 Tauri 版用 product_name（PairDesk_）。两种都匹配，避免脚本失效。
  URL="$(get_url "[Pp]air[Dd]esk[-A-Za-z]*_[0-9][0-9.]*_${ARCH_DEB}\.deb")"
  [ -z "$URL" ] && { echo "❌ release 中无 ${ARCH_DEB} 的 .deb"; exit 1; }
  echo "     $URL"

  echo "==> [2/3] 下载并安装"
  TMP="$(mktemp /tmp/PairDesk.XXXXXX.deb)"
  trap 'rm -f "$TMP"' EXIT
  curl -fsSL "$URL" -o "$TMP"
  apt-get update -qq && apt-get install -y -qq "$TMP"   # 自动补依赖

  echo "==> [3/3] 完成"
  echo " ✅ PairDesk 已安装（dpkg -l | grep pairdesk 可查）"
  echo "    启动: pairdesk  /  卸载: apt remove pairdesk"
else
  # ---------- AppImage 免 root ----------
  echo "==> [1/3] 解析 .AppImage 下载地址"
  # 同 .deb：兼容 pairdesk-ui_ 与 PairDesk_ 两种前缀
  URL="$(get_url "[Pp]air[Dd]esk[-A-Za-z]*_[0-9][0-9.]*_${ARCH_APPIMAGE}\.AppImage")"
  [ -z "$URL" ] && { echo "❌ release 中无 ${ARCH_APPIMAGE} 的 .AppImage"; exit 1; }
  echo "     $URL"

  echo "==> [2/3] 下载到 ~/.local/bin"
  mkdir -p "$HOME/.local/bin"
  DEST="$HOME/.local/bin/PairDesk.AppImage"
  curl -fsSL "$URL" -o "$DEST"
  chmod +x "$DEST"

  echo "==> [3/3] 完成"
  echo " ✅ 已安装: $DEST"
  echo "    启动: $DEST   （若报 fuse 错误: sudo apt install libfuse2）"
  echo "    或把 ~/.local/bin 加进 PATH 后直接输 PairDesk.AppImage"
fi
