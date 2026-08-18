#!/usr/bin/env bash
# 双进程 QUIC 点对点验证驱动：起 host 后台 + viewer 前台，收集两端输出。
cd /home/debian/code/pairdesk
BIN=./target/debug/examples/quic_p2p
PORT=29501
"$BIN" host 127.0.0.1:$PORT >/tmp/q2p-host.log 2>&1 &
HPID=$!
sleep 2
"$BIN" viewer 127.0.0.1:$PORT >/tmp/q2p-viewer.log 2>&1
VRC=$?
kill $HPID 2>/dev/null; wait $HPID 2>/dev/null
echo "=== viewer 输出 ==="; cat /tmp/q2p-viewer.log
echo "=== host 输出 ==="; cat /tmp/q2p-host.log
echo "viewer_exit=$VRC"
