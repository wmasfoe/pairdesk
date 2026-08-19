#!/usr/bin/env bash
# ============================================================
# PairDesk macOS 一键安装
#
# 无需 Apple 签名也能用：脚本自动解除 quarantine 隔离属性，
# 装完直接双击打开，不会再报"已损坏"或拦截。
#
# 用法（一条命令）：
#   curl -fsSL https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-macos.sh | bash
#
# 可选：装完自动打开
#   curl -fsSL ... | bash -s -- --open
# ============================================================
set -euo pipefail

REPO="wmasfoe/pairdesk"
APP_NAME="PairDesk"
OPEN_APP=false
[ "${1:-}" = "--open" ] && OPEN_APP=true

# ---------- 架构 → 产物匹配 ----------
case "$(uname -m)" in
  arm64)  DMG_PATTERN="aarch64\.dmg" ;;   # Apple Silicon
  x86_64) DMG_PATTERN="x86_64\.dmg" ;;
  *) echo "❌ 不支持的架构: $(uname -m)"; exit 1 ;;
esac

echo "==> [1/5] 解析最新 release 的 .dmg 下载地址"
URL="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep -oE "https://[^\"]*${DMG_PATTERN}\"" | tr -d '"' | head -1)"
if [ -z "$URL" ]; then
  echo "❌ 最新 release 中没有找到 ${DMG_PATTERN} 的安装包（Intel 版可能尚未发布）"
  exit 1
fi
echo "     $URL"

DMG_FILE="$(mktemp /tmp/PairDesk.XXXXXX.dmg)"
trap 'rm -f "$DMG_FILE"' EXIT

echo "==> [2/5] 下载 dmg"
curl -fSL --progress-bar "$URL" -o "$DMG_FILE"

echo "==> [3/5] 挂载并复制到 /Applications"
MOUNT="$(hdiutil attach -nobrowse -readonly "$DMG_FILE" | tail -1 | awk '{print $NF}')"
trap 'hdiutil detach "$MOUNT" -quiet 2>/dev/null || true; rm -f "$DMG_FILE"' EXIT
rm -rf "/Applications/${APP_NAME}.app"
cp -R "${MOUNT}/${APP_NAME}.app" /Applications/

echo "==> [4/5] 解除隔离（未签名应用打开不再报"已损坏"）"
xattr -dr com.apple.quarantine "/Applications/${APP_NAME}.app" 2>/dev/null || true

echo "==> [5/5] 清理"
hdiutil detach "$MOUNT" -quiet 2>/dev/null || true
trap - EXIT
rm -f "$DMG_FILE"

echo
echo "==============================================="
echo " ✅ PairDesk 已安装到 /Applications/PairDesk.app"
if $OPEN_APP; then
  echo "    正在打开…"
  open "/Applications/${APP_NAME}.app"
else
  echo "    现在打开：open /Applications/PairDesk.app"
fi
echo "    首次使用需在 系统设置→隐私与安全 授予:"
echo "      屏幕录制（被控端画面） + 辅助功能（控制端注入）"
echo "==============================================="
