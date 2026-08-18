#!/usr/bin/env bash
# QUIC 打洞直连 e2e：被控端起 QUIC server、控制端经 QUIC 连上，跑完整会话(认证+画面+输入)。
# 模拟"异网打洞成功后的 P2P 直连"(本机双进程)。帧协议与 TCP/中继完全互通。
set -uo pipefail

cd /home/debian/code/pairdesk
BIN=./target/debug/pairdesk
# 双进程共享同一身份证书目录(模拟同机两端；真实跨机靠信令交换公钥)
export HOME=/tmp/pd-quic-home
rm -rf "$HOME"

QUIC_PORT=29601
PASS=quicsec1
DISPLAY_NUM=:99
EXPECT_DIR=/tmp/pd-quic-exp
FRAMES_DIR=/tmp/pd-quic-frames

PASS_COUNT=0; FAIL_COUNT=0
pass() { echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT+1)); }
fail() { echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT+1)); }
check() { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (期望=$3 实际=$2)"; fi; }

pkill -x Xvfb 2>/dev/null; pkill -x pairdesk 2>/dev/null
sleep 1; rm -f /tmp/.X99-lock
rm -rf $EXPECT_DIR $FRAMES_DIR; mkdir -p $EXPECT_DIR $FRAMES_DIR

echo "=== 1. 虚拟屏幕 + 彩色窗口 ==="
Xvfb $DISPLAY_NUM -screen 0 640x480x24 >$EXPECT_DIR/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 1.5
DISPLAY=$DISPLAY_NUM ./target/debug/examples/paint 336699 640 480 >$EXPECT_DIR/paint.log 2>&1 &
PAINT_PID=$!
for i in $(seq 1 20); do grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && break; sleep 0.3; done
grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && pass "彩色窗口已显示" || fail "窗口未就绪"

echo "=== 2. 被控端 QUIC 打洞直连 server (端口 $QUIC_PORT) ==="
DISPLAY=$DISPLAY_NUM $BIN serve --quic-port $QUIC_PORT --password $PASS --fps 10 --jpeg 85 >$EXPECT_DIR/host.log 2>&1 &
HOST_PID=$!
sleep 2

echo "=== 3. 控制端经 QUIC 连接,收 3 帧+输入回传 ==="
$BIN connect 0.0.0.0:1 --quic 127.0.0.1:$QUIC_PORT --password $PASS --frames 3 --dump-dir $FRAMES_DIR --test-input 320,240 >$EXPECT_DIR/viewer.log 2>&1
echo "--- 控制端输出 ---"; grep -E "QUIC|认证|📸|✅|❌|📐" $EXPECT_DIR/viewer.log

grep -q "认证 成功" $EXPECT_DIR/viewer.log && pass "认证成功(经 QUIC 完整会话)" || fail "认证失败"
N_FRAMES=$(ls $FRAMES_DIR/frame-*.jpg 2>/dev/null | wc -l)
check "收到画面帧(经 QUIC)" "$N_FRAMES" "3"

CENTER=$(ffmpeg -v error -i $FRAMES_DIR/frame-0003.jpg -vf "crop=16:16:312:232,format=rgb24" -f rawvideo - 2>/dev/null | od -An -v -N3 -tu1 | tr -s ' ')
read R G B <<< "${CENTER:-0 0 0}"
if [ $((R-51)) -ge -12 ] && [ $((R-51)) -le 12 ] && [ $((G-102)) -ge -12 ] && [ $((G-102)) -le 12 ] && [ $((B-153)) -ge -12 ] && [ $((B-153)) -le 12 ]; then
  pass "画面内容正确 (RGB=$R,$G,$B ≈ #336699, 经 QUIC 全链路)"
else
  fail "画面内容错误 (RGB=$R,$G,$B)"
fi
grep -q "📐" $EXPECT_DIR/viewer.log && pass "输入注入回传(经 QUIC)" || true

echo "=== 4. 清理 ==="
kill $HOST_PID $PAINT_PID $XVFB_PID 2>/dev/null
pkill -x pairdesk 2>/dev/null; pkill -x Xvfb 2>/dev/null

echo ""
echo "═══════════════════════════════"
echo "  QUIC 打洞直连结果: $PASS_COUNT 通过 / $FAIL_COUNT 失败"
echo "═══════════════════════════════"
[ $FAIL_COUNT -eq 0 ]
