#!/usr/bin/env bash
# ============================================================
# PairDesk relay 一键部署脚本（VPS）
#
# 用途：从 GitHub Releases 拉取最新 pairdesk-relay 二进制，
#       安装到 /usr/local/bin 并注册 systemd 服务（开机自启 + 崩溃重启）。
#
# 用法（在 VPS 上）：
#   sudo bash deploy-relay.sh            # 默认端口 8977
#   sudo bash deploy-relay.sh 9999       # 指定端口
#
# 验证：
#   systemctl status pairdesk-relay@8977
#   ss -ltn | grep 8977
# ============================================================
set -euo pipefail

PORT="${1:-8977}"
REPO="wmasfoe/pairdesk"

# ---------- 架构 → 资产名 ----------
case "$(uname -m)" in
  x86_64|amd64)  ASSET="pairdesk-relay-Linux-x86_64" ;;
  aarch64|arm64) ASSET="pairdesk-relay-Linux-aarch64" ;;
  *) echo "❌ 不支持的架构: $(uname -m)（目前发布产物为 x86_64 / aarch64）"; exit 1 ;;
esac

echo "==> [1/4] 获取最新 release 资产: $ASSET"
API="https://api.github.com/repos/${REPO}/releases/latest"
URL="$(curl -fsSL "$API" | grep -oE "https://[^\"]*${ASSET}\"" | tr -d '"' | head -1)"
if [ -z "$URL" ]; then
  echo "❌ 未在最新 release 中找到 $ASSET（可先 gh release 检查，或改脚本指向具体版本）"
  exit 1
fi

echo "==> [2/4] 下载并安装到 /usr/local/bin/pairdesk-relay"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP"
chmod +x "$TMP"
install -m 0755 "$TMP" /usr/local/bin/pairdesk-relay
/usr/local/bin/pairdesk-relay --version 2>/dev/null || echo "（--version 无输出属正常，二进制已就位）"

echo "==> [3/4] 注册 systemd 服务（模板实例: pairdesk-relay@${PORT}）"
UNIT="/etc/systemd/system/pairdesk-relay@.service"
cat > "$UNIT" <<'EOF'
[Unit]
Description=PairDesk Relay (P2P 中继服务器) — 端口 %i
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/pairdesk-relay %i
Restart=always
RestartSec=3
# 若 relay 只做信令/转发，无落盘需求，可加: NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable "pairdesk-relay@${PORT}" --now

echo "==> [4/4] 验证"
sleep 1
systemctl --no-pager --lines=5 status "pairdesk-relay@${PORT}" || true
if ss -ltn 2>/dev/null | grep -q ":${PORT} "; then
  echo "✅ relay 已监听 :${PORT}"
else
  echo "⚠️ 未检测到端口监听，请看上面状态输出排查（也可能 ss 未安装）"
fi

PUBLIC_IP="$(curl -fsSL --max-time 5 https://api.ipify.org 2>/dev/null || echo '你的公网IP')"
echo
echo "==============================================="
echo " ✅ PairDesk relay 部署完成"
echo "    端口:      ${PORT}"
echo "    公网地址:  ${PUBLIC_IP}:${PORT}"
echo "    服务:      systemctl status pairdesk-relay@${PORT}"
echo "    日志:      journalctl -u pairdesk-relay@${PORT} -f"
echo "  客户端填写:  被控端与控制端的「中继/VPS 地址」都填 ${PUBLIC_IP}:${PORT}"
echo "  防火墙:      若 VPS 有 ufw/firewalld，放行: ufw allow ${PORT}/tcp"
echo "==============================================="
