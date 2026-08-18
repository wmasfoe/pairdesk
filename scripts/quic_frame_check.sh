#!/usr/bin/env bash
# QUIC 承载帧协议双进程验证驱动。
cd /home/debian/code/pairdesk
BIN=./target/debug/examples/quic_frame_p2p
PORT=29601
rm -rf /tmp/pd-quic
"$BIN" host 127.0.0.1:$PORT >/tmp/qf-host.log 2>&1 &
HPID=$!
sleep 2
"$BIN" viewer 127.0.0.1:$PORT >/tmp/qf-viewer.log 2>&1
VRC=$?
kill $HPID 2>/dev/null; wait $HPID 2>/dev/null
echo "=== viewer 输出 ==="; cat /tmp/qf-viewer.log
echo "=== host 输出 ==="; cat /tmp/qf-host.log
echo "viewer_exit=$VRC"
