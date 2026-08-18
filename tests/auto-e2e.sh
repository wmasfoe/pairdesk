#!/usr/bin/env bash
# 自动择一 e2e：验证"无感择优 + 自动降级"
#  场景A: host 走 auto(QUIC+中继) → viewer auto 应自动选 QUIC 打洞直连
#  场景B: host 仅中继(无 QUIC server) → viewer auto 自动降级到中继兜底
set -uo pipefail
cd /home/debian/code/pairdesk
BIN=./target/debug/pairdesk
export HOME=/tmp/pd-quic-home; rm -rf "$HOME"
RELAY_PORT=8977; SID=autosess; PASS=autosec1; HOLE=8889; DISPLAY_NUM=:99
EXPECT_DIR=/tmp/pd-auto-exp; FRAMES=/tmp/pd-auto-frames
PASS_COUNT=0; FAIL_COUNT=0
pass(){ echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT+1)); }
fail(){ echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT+1)); }
check(){ if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (期望=$3 实际=$2)"; fi; }

clean(){ pkill -x pairdesk 2>/dev/null; pkill -x pairdesk-relay 2>/dev/null; pkill -x Xvfb 2>/dev/null; sleep 1; rm -f /tmp/.X99-lock /tmp/.X${DISPLAY_NUM#:}-lock 2>/dev/null; }
clean2(){ pkill -x pairdesk 2>/dev/null; pkill -x Xvfb 2>/dev/null; sleep 1; rm -f /tmp/.X99-lock; }
setup_display(){
  Xvfb $DISPLAY_NUM -screen 0 640x480x24 >$EXPECT_DIR/xvfb.log 2>&1 &
  XVFB_PID=$!; sleep 1.5
  DISPLAY=$DISPLAY_NUM ./target/debug/examples/paint 336699 640 480 >$EXPECT_DIR/paint.log 2>&1 &
  PAINT_PID=$!
  for i in $(seq 1 20); do grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && break; sleep 0.3; done
}
color_ok(){
  local f=$1
  local c=$(ffmpeg -v error -i $f -vf "crop=16:16:312:232,format=rgb24" -f rawvideo - 2>/dev/null | od -An -v -N3 -tu1 | tr -s ' ')
  read R G B <<< "${c:-0 0 0}"
  if [ $((R-51)) -ge -12 ] && [ $((R-51)) -le 12 ] && [ $((G-102)) -ge -12 ] && [ $((G-102)) -le 12 ] && [ $((B-153)) -ge -12 ] && [ $((B-153)) -le 12 ]; then return 0; else echo "(RGB=$R,$G,$B)"; return 1; fi
}

mkdir -p $EXPECT_DIR $FRAMES

## ===== 场景 A：QUIC 可达 → viewer 自动走 QUIC =====
echo "=== 场景A host auto(QUIC+中继), viewer auto 应选 QUIC ==="
clean
./target/debug/pairdesk-relay $RELAY_PORT >$EXPECT_DIR/relayA.log 2>&1 &
RELAY_PID=$!; sleep 1
setup_display
DISPLAY=$DISPLAY_NUM $BIN serve --auto --relay 127.0.0.1:$RELAY_PORT --sid $SID --hole-port $HOLE --password $PASS --fps 10 --jpeg 85 >$EXPECT_DIR/hostA.log 2>&1 &
HPID=$!; sleep 2
$BIN connect 0.0.0.0:1 --auto --relay 127.0.0.1:$RELAY_PORT --sid $SID --password $PASS --frames 3 --dump-dir $FRAMES/A >$EXPECT_DIR/viewerA.log 2>&1
grep -q "传输路径: QUIC 打洞直连" $EXPECT_DIR/viewerA.log && pass "A: viewer 自动选 QUIC 打洞直连" || fail "A: 未走 QUIC(路径=$(grep 传输路径 $EXPECT_DIR/viewerA.log))"
grep -q "认证 成功" $EXPECT_DIR/viewerA.log && pass "A: 认证成功(经自动选路)" || fail "A: 认证失败"
N=$(ls $EXPECT_DIR/../pd-auto-frames/A/frame-*.jpg 2>/dev/null | wc -l) 2>/dev/null
check "A: 收到画面帧" "$(ls $FRAMES/A/frame-*.jpg 2>/dev/null | wc -l)" "3"
if color_ok $FRAMES/A/frame-0003.jpg; then pass "A: 画面内容正确(经自动择一)"; else fail "A: 画面内容错误 $c"; fi
kill $HPID $PAINT_PID $XVFB_PID $RELAY_PID 2>/dev/null

## ===== 场景 B：QUIC 不可达 → viewer auto 自动降级中继 =====
echo "=== 场景B host 仅中继(无 QUIC server), viewer auto 应降级中继兜底 ==="
clean2
./target/debug/pairdesk-relay $RELAY_PORT >$EXPECT_DIR/relayB.log 2>&1 &
RELAY_PID=$!; sleep 1
setup_display
DISPLAY=$DISPLAY_NUM $BIN serve --relay 127.0.0.1:$RELAY_PORT --sid $SID --hole-port $HOLE --password $PASS --fps 10 --jpeg 85 >$EXPECT_DIR/hostB.log 2>&1 &
HPID=$!; sleep 2
$BIN connect 0.0.0.0:1 --auto --relay 127.0.0.1:$RELAY_PORT --sid $SID --password $PASS --frames 3 --dump-dir $FRAMES/B >$EXPECT_DIR/viewerB.log 2>&1
grep -q "传输路径: 中继兜底" $EXPECT_DIR/viewerB.log && pass "B: viewer 自动降级到中继兜底" || fail "B: 未走中继(路径=$(grep 传输路径 $EXPECT_DIR/viewerB.log))"
grep -q "认证 成功" $EXPECT_DIR/viewerB.log && pass "B: 认证成功(经中继兜底)" || fail "B: 认证失败"
check "B: 收到画面帧" "$(ls $FRAMES/B/frame-*.jpg 2>/dev/null | wc -l)" "3"
if color_ok $FRAMES/B/frame-0003.jpg; then pass "B: 画面内容正确(经中继兜底)"; else fail "B: 画面内容错误 $c"; fi
kill $HPID $PAINT_PID $XVFB_PID $RELAY_PID 2>/dev/null
clean2

echo ""
echo "═══════════════════════════════"
echo "  自动择一结果: $PASS_COUNT 通过 / $FAIL_COUNT 失败"
echo "═══════════════════════════════"
[ $FAIL_COUNT -eq 0 ]
