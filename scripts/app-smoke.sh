#!/usr/bin/env bash
# Tauri 壳无头冒烟：在 Xvfb 里启动 App，确认进程保持运行（能加载前端 + 进入事件循环）。
# 低成本的"壳能跑"验证（不是全链路 e2e，那在下一步用自测模式做）。
set -uo pipefail
cd /home/debian/code/pairdesk
mkdir -p /tmp/app-smoke
pkill -x Xvfb 2>/dev/null; pkill -x pairdesk-app 2>/dev/null; sleep 1; rm -f /tmp/.X99-lock
Xvfb :99 -screen 0 1024x768x24 >/tmp/app-smoke/xvfb.log 2>&1 &
XV=$!
sleep 1.5
export DISPLAY=:99
timeout 9 ./target/debug/pairdesk-app >/tmp/app-smoke/app.log 2>&1 &
APP=$!
sleep 5
if kill -0 $APP 2>/dev/null; then
  echo "PASS: Tauri 壳在 Xvfb 下保持运行(5s)"
else
  echo "FAIL: 壳启动后退出"
fi
echo "--- app.log ---"
cat /tmp/app-smoke/app.log 2>/dev/null | head -25
kill $APP $XV 2>/dev/null
pkill -x pairdesk-app 2>/dev/null; pkill -x Xvfb 2>/dev/null
