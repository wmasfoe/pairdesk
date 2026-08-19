#!/usr/bin/env bash
# todo5 全链路自测：两个 Tauri App 实例(host+viewer) 经 relay, 输入会话码+密码,
# 控制端收帧落盘 → 验证"壳 + 内核"完整链路(host采集→内核→App event→收帧)。
# 用 App 自测模式(PD_ROLE)驱动, 绕开 UI 点击。
set -uo pipefail
cd /home/debian/code/pairdesk
APP=./target/debug/pairdesk-app
RELAY_PORT=8977; SID=appsess; PASS=appsec1; HOLE=8889; DISPLAY_NUM=:99
EXPECT_DIR=/tmp/pd-app-exp; FRAMES=/tmp/pd-app-frames
PASS_COUNT=0; FAIL_COUNT=0
pass(){ echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT+1)); }
fail(){ echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT+1)); }
color_ok(){
  local f=$1
  local c=$(ffmpeg -v error -i $f -vf "crop=16:16:312:232,format=rgb24" -f rawvideo - 2>/dev/null | od -An -v -N3 -tu1 | tr -s ' ')
  read R G B <<< "${c:-0 0 0}"
  if [ $((R-51)) -ge -12 ] && [ $((R-51)) -le 12 ] && [ $((G-102)) -ge -12 ] && [ $((G-102)) -le 12 ] && [ $((B-153)) -ge -12 ] && [ $((B-153)) -le 12 ]; then return 0; else echo "(RGB=$R,$G,$B)"; return 1; fi
}

pkill -x Xvfb 2>/dev/null; pkill -x pairdesk-app 2>/dev/null; pkill -x pairdesk-relay 2>/dev/null
sleep 1; rm -f /tmp/.X99-lock
rm -rf $EXPECT_DIR $FRAMES; mkdir -p $EXPECT_DIR $FRAMES

echo "=== 1. 虚拟屏幕 + 彩色窗口(被采集的'屏幕') ==="
Xvfb $DISPLAY_NUM -screen 0 640x480x24 >$EXPECT_DIR/xvfb.log 2>&1 &
XV=$!; sleep 1.5
DISPLAY=$DISPLAY_NUM ./target/debug/examples/paint 336699 640 480 >$EXPECT_DIR/paint.log 2>&1 &
PAINT=$!
for i in $(seq 1 20); do grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && break; sleep 0.3; done
grep -q "已绘制" $EXPECT_DIR/paint.log && pass "彩色窗口已显示" || fail "窗口未就绪"

echo "=== 2. 起 relay 中继 ==="
./target/debug/pairdesk-relay $RELAY_PORT >$EXPECT_DIR/relay.log 2>&1 &
RELAY=$!; sleep 1

echo "=== 3. 起「被控端」App(自测模式, 会话码=$SID) ==="
DISPLAY=$DISPLAY_NUM PD_ROLE=host PD_RELAY=127.0.0.1:$RELAY_PORT PD_SID=$SID PD_PASSWORD=$PASS PD_HOLE=$HOLE $APP >$EXPECT_DIR/host.log 2>&1 &
HAPP=$!
sleep 2
grep -q "被控端已启动" $EXPECT_DIR/host.log && pass "被控端 App 启动并起会话" || fail "被控端启动失败(见 host.log)"

echo "=== 4. 起「控制端」App(自测模式, 收帧落盘) ==="
DISPLAY=$DISPLAY_NUM PD_ROLE=viewer PD_RELAY=127.0.0.1:$RELAY_PORT PD_SID=$SID PD_PASSWORD=$PASS PD_DUMP_DIR=$FRAMES $APP >$EXPECT_DIR/viewer.log 2>&1 &
VAPP=$!
# 等待收到至少 1 帧
for i in $(seq 1 30); do [ "$(ls $FRAMES/frame-*.jpg 2>/dev/null | wc -l)" -ge 3 ] && break; sleep 0.5; done
grep -q "控制端已启动" $EXPECT_DIR/viewer.log && pass "控制端 App 启动并起会话" || fail "控制端启动失败"
N=$(ls $FRAMES/frame-*.jpg 2>/dev/null | wc -l)
[ "$N" -ge 3 ] && pass "控制端收到画面帧经 App 全链路(共 $N 帧)" || fail "未收到>=3帧(实际 $N)"
LAST=$(ls $FRAMES/frame-*.jpg 2>/dev/null | tail -1)
if [ -n "$LAST" ] && color_ok $LAST; then pass "画面内容正确(≈#336699, App 全链路)"; else fail "画面内容错误"; fi

echo "=== 5. 清理 ==="
kill $VAPP $HAPP $RELAY $PAINT $XV 2>/dev/null
pkill -x pairdesk-app 2>/dev/null; pkill -x pairdesk-relay 2>/dev/null; pkill -x Xvfb 2>/dev/null

echo ""
echo "═══════════════════════════════"
echo "  App 全链路自测: $PASS_COUNT 通过 / $FAIL_COUNT 失败"
echo "═══════════════════════════════"
[ $FAIL_COUNT -eq 0 ]
