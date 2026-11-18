#!/usr/bin/env bash
# 前置：需要先构建测试辅助二进制（含 Linux 专有的 examples/paint）：
#   ./scripts/build-test-bins.sh

# 手动诊断：paint 窗口是否真的能通过 GetImage 读到
cd "$(dirname "$0")/.." || exit 1
pkill -x Xvfb 2>/dev/null; rm -f /tmp/.X99-lock; sleep 1
Xvfb :99 -screen 0 640x480x24 >/tmp/xvfb.log 2>&1 &
XPID=$!
sleep 2
DISPLAY=:99 ./target/debug/examples/paint 336699 640 480 >/tmp/paint.log 2>&1 &
PPID=$!
sleep 2
echo "--- paint 日志 ---"; cat /tmp/paint.log
echo "--- 探针: 中心(320,240) ---"
DISPLAY=:99 ./target/debug/examples/capture_probe 320 240
echo "--- 探针: 左上(0,0) ---"
DISPLAY=:99 ./target/debug/examples/capture_probe 0 0
echo "--- 探针: 右下(639,479) ---"
DISPLAY=:99 ./target/debug/examples/capture_probe 639 479
kill $PPID $XPID 2>/dev/null
wait 2>/dev/null