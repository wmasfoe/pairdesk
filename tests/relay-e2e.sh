#!/usr/bin/env bash
# 中继链路 e2e 验收：验证"异地跨网"场景（两台在不同 WiFi，经 VPS 中继互连）。
#
# 流程：起 relay(本机模拟 VPS) + 虚拟屏幕 + 画色块窗口
#   → 被控端经中继注册 → 控制端经中继匹配 → 走既有握手 → 收帧断言颜色。
# 这证明透明桥接不解析协议，客户端现有握手/加密/画面/输入全链路在中继下照常。
set -uo pipefail

cd /home/debian/code/pairdesk
BIN=./target/debug/pairdesk
RELAY=./target/debug/pairdesk-relay
RELAY_PORT=8989
SID=testroom001
PASS=relay123
DISPLAY_NUM=:99
EXPECT_DIR=/tmp/pd-relay-exp
FRAMES_DIR=/tmp/pd-relay-frames

PASS_COUNT=0; FAIL_COUNT=0
pass() { echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT+1)); }
fail() { echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT+1)); }
check() { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (期望=$3 实际=$2)"; fi; }

pkill -x Xvfb 2>/dev/null; pkill -x pairdesk 2>/dev/null; pkill -x pairdesk-relay 2>/dev/null
sleep 1; rm -f /tmp/.X99-lock
rm -rf $EXPECT_DIR $FRAMES_DIR; mkdir -p $EXPECT_DIR $FRAMES_DIR

echo "=== 1. 启动中继服务器 (模拟 VPS) :$RELAY_PORT ==="
$RELAY $RELAY_PORT >$EXPECT_DIR/relay.log 2>&1 &
RELAY_PID=$!
sleep 1
kill -0 $RELAY_PID 2>/dev/null && pass "relay 运行中 (pid $RELAY_PID)" || { fail "relay 启动失败"; cat $EXPECT_DIR/relay.log; exit 1; }

echo "=== 2. 虚拟屏幕 + 彩色桌面窗口 ==="
Xvfb $DISPLAY_NUM -screen 0 640x480x24 >$EXPECT_DIR/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 1.5
for i in $(seq 1 20); do grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && break; sleep 0.3; done
DISPLAY=$DISPLAY_NUM $BIN --help >/dev/null 2>&1 &
sleep 0.3
DISPLAY=$DISPLAY_NUM ./target/debug/examples/paint 336699 640 480 >$EXPECT_DIR/paint.log 2>&1 &
PAINT_PID=$!
sleep 1.5
grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && pass "彩色窗口已显示" || fail "窗口未就绪"

echo "=== 3. 被控端经中继注册 (sid=$SID) ==="
DISPLAY=$DISPLAY_NUM $BIN serve --relay 127.0.0.1:$RELAY_PORT --sid $SID --password $PASS --fps 10 --jpeg 85 >$EXPECT_DIR/host.log 2>&1 &
HOST_PID=$!
sleep 1.2
# relay 应记录 host 注册
sleep 0.8
grep -q "host 注册" $EXPECT_DIR/relay.log && pass "relay 收到 host 注册" || fail "relay 未收到 host 注册"

echo "=== 4. 控制端经中继连接,收 3 帧 ==="
$BIN connect 127.0.0.1:8888 --relay 127.0.0.1:$RELAY_PORT --sid $SID --password $PASS --frames 3 --dump-dir $FRAMES_DIR --test-input 320,240 >$EXPECT_DIR/viewer.log 2>&1
echo "--- 控制端输出 ---"; grep -E "经中继|认证|📸|✅|❌|📐" $EXPECT_DIR/viewer.log

grep -q "认证 成功" $EXPECT_DIR/viewer.log && pass "认证成功(经中继透明桥接)" || fail "认证失败"
grep -q "viewer 匹配" $EXPECT_DIR/relay.log && pass "relay 完成双方桥接" || fail "relay 未桥接"
N_FRAMES=$(ls $FRAMES_DIR/frame-*.jpg 2>/dev/null | wc -l)
check "收到画面帧(经中继流转)" "$N_FRAMES" "3"

CENTER=$(ffmpeg -v error -i $FRAMES_DIR/frame-0003.jpg -vf "crop=16:16:312:232,format=rgb24" -f rawvideo - 2>/dev/null | od -An -v -N3 -tu1 | tr -s ' ')
read R G B <<< "${CENTER:-0 0 0}"
if [ $((R-51)) -ge -12 ] && [ $((R-51)) -le 12 ] && [ $((G-102)) -ge -12 ] && [ $((G-102)) -le 12 ] && [ $((B-153)) -ge -12 ] && [ $((B-153)) -le 12 ]; then
  pass "画面内容正确 (RGB=$R,$G,$B ≈ #336699, 经中继全链路)"
else
  fail "画面内容错误 (RGB=$R,$G,$B)"
fi

echo "=== 5. 清理 ==="
kill $HOST_PID $PAINT_PID $XVFB_PID $RELAY_PID 2>/dev/null
pkill -x pairdesk 2>/dev/null; pkill -x pairdesk-relay 2>/dev/null; pkill -x Xvfb 2>/dev/null

echo ""
echo "═══════════════════════════════"
echo "  中继链路结果: $PASS_COUNT 通过 / $FAIL_COUNT 失败"
echo "═══════════════════════════════"
[ $FAIL_COUNT -eq 0 ]
