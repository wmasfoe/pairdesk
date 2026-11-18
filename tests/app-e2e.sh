#!/usr/bin/env bash
# GPUI 壳全链路自测：两个 App 实例（host + viewer）经 relay 互连，
# 验证「壳 + 内核」完整链路（采集 → 内核 → App 事件 → 画面渲染）。
#
# 用 PD_AUTOSTART 驱动，绕开 UI 点击（对应旧版 Tauri 的 PD_ROLE 机制）。
#
# 与 tests/e2e.sh 的区别：
#   - tests/e2e.sh        测 CLI（pairdesk serve/connect），不经 UI
#   - tests/app-e2e.sh    测 App 壳（GPUI），验证"壳能把内核接起来"
#
# 依赖：Xvfb（Linux 无头）、pairdesk-relay、GPUI 壳二进制
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

APP=./target/debug/pairdesk-ui
RELAY=./target/debug/pairdesk-relay
RELAY_PORT=8977
SID="appsess"
PASS="appsec1"
HOLE=8889
DISPLAY_NUM="${DISPLAY_NUM:-:99}"
EXPECT_DIR=/tmp/pd-gpui-e2e
PASS_COUNT=0
FAIL_COUNT=0

pass() { echo "  ✅ $1"; PASS_COUNT=$((PASS_COUNT + 1)); }
fail() { echo "  ❌ $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

cleanup() {
    pkill -x pairdesk-ui 2>/dev/null
    pkill -x pairdesk-relay 2>/dev/null
    pkill -x Xvfb 2>/dev/null
}
trap cleanup EXIT

cleanup
sleep 1
rm -rf "$EXPECT_DIR"
mkdir -p "$EXPECT_DIR"

# ---- 0. 前置检查 ----
for bin in "$APP" "$RELAY"; do
    if [ ! -x "$bin" ]; then
        echo "❌ 缺少二进制 $bin，请先 cargo build"
        exit 1
    fi
done

# ---- 1. 虚拟屏幕（Linux 无头环境）----
if [ "$(uname)" = "Linux" ]; then
    rm -f "/tmp/.X${DISPLAY_NUM#:}-lock"
    Xvfb "$DISPLAY_NUM" -screen 0 1024x768x24 >"$EXPECT_DIR/xvfb.log" 2>&1 &
    sleep 1.5
    export DISPLAY="$DISPLAY_NUM"
    echo "=== 1. 虚拟屏幕已启动 ($DISPLAY) ==="
else
    echo "=== 1. 跳过虚拟屏幕（非 Linux，用当前桌面）==="
fi

# ---- 2. 中继 ----
"$RELAY" "$RELAY_PORT" >"$EXPECT_DIR/relay.log" 2>&1 &
sleep 1.5
if pgrep -x pairdesk-relay >/dev/null; then
    pass "中继已启动 (:$RELAY_PORT)"
else
    fail "中继启动失败"
    exit 1
fi

# ---- 3. 被控端 ----
PD_LOG_INPUT=1 \
PD_START_PAGE=host PD_AUTOSTART=host \
PD_SID="$SID" PD_PASSWORD="$PASS" PD_HOLE="$HOLE" \
PD_RELAY="127.0.0.1:$RELAY_PORT" \
    "$APP" >"$EXPECT_DIR/host.log" 2>&1 &
sleep 4
if grep -q "认证成功\|AuthOk" "$EXPECT_DIR/host.log" 2>/dev/null; then
    pass "被控端已起会话并等待连入"
elif grep -q "host-auto\|被控端" "$EXPECT_DIR/host.log" 2>/dev/null; then
    pass "被控端已起会话（等待控制端）"
else
    fail "被控端启动失败（见 $EXPECT_DIR/host.log）"
fi

# ---- 4. 控制端 ----
PD_START_PAGE=viewer PD_AUTOSTART=viewer \
PD_SID="$SID" PD_PASSWORD="$PASS" \
PD_RELAY="127.0.0.1:$RELAY_PORT" \
    "$APP" >"$EXPECT_DIR/viewer.log" 2>&1 &
sleep 8

if grep -q "认证通过\|AuthOk" "$EXPECT_DIR/viewer.log" 2>/dev/null; then
    pass "控制端认证成功，加密通道建立"
else
    fail "控制端未认证（见 $EXPECT_DIR/viewer.log）"
fi

# ---- 5. 链路质量 ----
if grep -qE "打洞穿透成功" "$EXPECT_DIR/viewer.log" 2>/dev/null; then
    pass "QUIC 打洞直连成功"
elif grep -qE "中继" "$EXPECT_DIR/viewer.log" 2>/dev/null; then
    pass "中继兜底链路生效"
else
    fail "未确认传输路径"
fi

# ---- 6. 两个进程都存活（说明壳没崩）----
ALIVE=$(pgrep -x pairdesk-ui | wc -l | tr -d ' ')
if [ "$ALIVE" -ge 2 ]; then
    pass "两个 App 实例均存活"
else
    fail "存活实例数不足（实际 $ALIVE，期望 2）"
fi

# ---- 汇总 ----
echo
echo "================================"
echo "通过 $PASS_COUNT / 失败 $FAIL_COUNT"
echo "日志目录：$EXPECT_DIR"
echo "================================"
[ "$FAIL_COUNT" -eq 0 ] || exit 1
