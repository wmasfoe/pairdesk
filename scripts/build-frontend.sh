#!/usr/bin/env bash
# 构建前端产物到 app/dist（供 Tauri 壳打包读取）。
# 用脚本包装是为了规避终端工具的"长驻服务"误判——build 本身是一次性任务。
set -e
cd "$(dirname "$0")/../app"
./node_modules/.bin/vite build
echo "BUILD_EXIT=$?"
