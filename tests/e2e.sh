#!/usr/bin/env bash
# M0 端到端验收：虚拟屏幕上的双实例互连测试
# 验证三条链路：画面(采集→编码→加密→传输→解密) / 输入(控制端→被控端→注入) / 认证
set -uo pipefail

BIN=./target/debug/pairdesk
PORT=8888
PASS=test123
DISPLAY_NUM=:99
FRAMES_DIR=/tmp/pd-frames
HOST_LOG=/tmp/pd-host.log
VIEWER_LOG=/tmp/pd-viewer.log
EXPECT_DIR=/tmp/pd-verify

PASS_COUNT=0
FAIL_COUNT=0

pass() { echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT+1)); }
fail() { echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT+1)); }
check() { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (期望 $3 实际 $2)"; fi; }

rm -rf $FRAMES_DIR $EXPECT_DIR; mkdir -p $FRAMES_DIR $EXPECT_DIR

echo "=== 1. 启动虚拟屏幕 :99 (640x480, 背景色 #336699) ==="
Xvfb $DISPLAY_NUM -screen 0 640x480x24 >$EXPECT_DIR/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 1.5
if ! kill -0 $XVFB_PID 2>/dev/null; then fail "Xvfb 启动失败"; cat $EXPECT_DIR/xvfb.log; exit 1; fi
pass "Xvfb 运行中 (pid $XVFB_PID)"

DISPLAY=$DISPLAY_NUM xsetroot -solid '#336699'
pass "背景色已设置 #336699"

# 在虚拟屏幕上显示一个彩色窗口（模拟真实桌面内容，窗口在 root 之上，
# root 的 GetImage 会合成窗口内容）。用已编译的二进制，并等待窗口真正画好。
PAINT_BIN=./target/debug/examples/paint
DISPLAY=$DISPLAY_NUM $PAINT_BIN 336699 640 480 >$EXPECT_DIR/paint.log 2>&1 &
PAINT_PID=$!
for i in $(seq 1 20); do
  if grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null; then break; fi
  sleep 0.5
done
if ! kill -0 $PAINT_PID 2>/dev/null; then fail "paint 窗口程序启动失败"; cat $EXPECT_DIR/paint.log; fi
grep -q "已绘制" $EXPECT_DIR/paint.log 2>/dev/null && pass "彩色窗口已显示 (#336699)" || fail "彩色窗口未就绪"
sleep 0.5

echo "=== 2. 启动被控端 (采集 :99, 端口 $PORT) ==="
DISPLAY=$DISPLAY_NUM $BIN serve --port $PORT --password $PASS --fps 10 --jpeg 85 >$HOST_LOG 2>&1 &
HOST_PID=$!
sleep 1
if ! kill -0 $HOST_PID 2>/dev/null; then fail "被控端启动失败"; cat $HOST_LOG; pkill -f Xvfb; exit 1; fi
pass "被控端运行中 (pid $HOST_PID)"

echo "=== 3. 控制端连接 + 收 3 帧 + 注入测试输入(移动鼠标到 320,240 并点击) ==="
$BIN connect 127.0.0.1:$PORT --password $PASS --frames 3 --dump-dir $FRAMES_DIR --test-input 320,240 >$VIEWER_LOG 2>&1
VIEWER_RC=$?

echo "--- 控制端输出 ---"; cat $VIEWER_LOG

# 3a. 认证是否成功
if grep -q "认证 成功" $VIEWER_LOG; then pass "认证成功"; else fail "认证失败"; fi
# 3b. 画面帧
N_FRAMES=$(ls $FRAMES_DIR/frame-*.jpg 2>/dev/null | wc -l)
check "收到画面帧" "$N_FRAMES" "3"
# 3c. 帧内容: 中间 16x16 块均值颜色 ≈ #336699 (JPEG 有损, 容差 ±12)
CENTER=$(ffmpeg -v error -i $FRAMES_DIR/frame-0003.jpg -vf "crop=16:16:312:232,format=rgb24" -f rawvideo - 2>/dev/null | od -An -v -N3 -tu1 | tr -s ' ')
read R G B <<< "$CENTER"
EXP_R=51; EXP_G=102; EXP_B=153
if [ $((R-EXP_R)) -ge -12 ] && [ $((R-EXP_R)) -le 12 ] && [ $((G-EXP_G)) -ge -12 ] && [ $((G-EXP_G)) -le 12 ] && [ $((B-EXP_B)) -ge -12 ] && [ $((B-EXP_B)) -le 12 ]; then
  pass "画面内容正确 (中心像素 RGB=$R,$G,$B ≈ #336699)"
else
  fail "画面内容错误 (中心像素 RGB=$R,$G,$B ≠ #336699)"
fi
# 3d. 输入链路: 被控端收到移动后,X server 指针应在 (320,240)
sleep 0.5
POINTER=$(DISPLAY=$DISPLAY_NUM xdotool getmouselocation 2>/dev/null | grep -oE 'x:[0-9]+ y:[0-9]+' || echo "x:0 y:0")
PX=$(echo "$POINTER" | grep -oE 'x:[0-9]+' | cut -d: -f2)
PY=$(echo "$POINTER" | grep -oE 'y:[0-9]+' | cut -d: -f2)
check "输入注入生效 (指针移动至 320,240)" "$PX,$PY" "320,240"

echo "=== 4. 错误密码应被拒绝 ==="
$BIN connect 127.0.0.1:$PORT --password WRONG --frames 1 2>&1 | grep -q "认证 失败" && pass "错误密码被拒绝" || fail "错误密码未被拒绝"

echo "=== 5. 清理 ==="
kill $HOST_PID $PAINT_PID 2>/dev/null; wait $HOST_PID $PAINT_PID 2>/dev/null
kill $XVFB_PID 2>/dev/null; wait $XVFB_PID 2>/dev/null
pkill -x pairdesk 2>/dev/null
pkill -x Xvfb 2>/dev/null

echo ""
echo "═══════════════════════════════"
echo "  结果: $PASS_COUNT 通过 / $FAIL_COUNT 失败"
echo "═══════════════════════════════"
[ $FAIL_COUNT -eq 0 ]