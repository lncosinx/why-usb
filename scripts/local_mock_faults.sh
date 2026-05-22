#!/usr/bin/env bash
set -euo pipefail

ADDR="${WHY_USB_FAULT_ADDR:-127.0.0.1:3025}"
CLIENT_TIMEOUT_SECONDS="${WHY_USB_CLIENT_TIMEOUT_SECONDS:-12s}"
LOG_DIR="${WHY_USB_FAULT_LOG_DIR:-$(mktemp -d)}"
DAEMON_LOG="$LOG_DIR/daemon.log"
CLIENT_LOG="$LOG_DIR/client.log"
DAEMON_PID=""

mkdir -p "$LOG_DIR"

cleanup() {
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
}

fail() {
    echo "local mock fault test failed: $*" >&2
    echo "--- daemon log: $DAEMON_LOG ---" >&2
    if [[ -f "$DAEMON_LOG" ]]; then
        sed -n '1,260p' "$DAEMON_LOG" >&2
    fi
    echo "--- client log: $CLIENT_LOG ---" >&2
    if [[ -f "$CLIENT_LOG" ]]; then
        sed -n '1,260p' "$CLIENT_LOG" >&2
    fi
    exit 1
}

trap cleanup EXIT

cargo build -p daemon -p client

env RUST_LOG=info \
    WHY_USB_ATTACH_DEVICE=1234:5678:1:2 \
    WHY_USB_MOCK_HID_KEYS=a,enter \
    WHY_USB_MOCK_TRANSFER_OUTCOMES=7=timeout,8=stall,9=short:4 \
    target/debug/daemon "$ADDR" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID="$!"

for _ in {1..80}; do
    if grep -q "listening for client connections" "$DAEMON_LOG"; then
        break
    fi

    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        fail "daemon exited before listening"
    fi

    sleep 0.1
done

grep -q "listening for client connections" "$DAEMON_LOG" \
    || fail "daemon did not start listening on $ADDR"

set +e
timeout "$CLIENT_TIMEOUT_SECONDS" env RUST_LOG=info \
    WHY_USB_MOCK_FRAME_LIMIT=3 WHY_USB_MOCK_FRAME_INTERVAL_MS=100 \
    target/debug/client "$ADDR" >"$CLIENT_LOG" 2>&1
CLIENT_STATUS="$?"
set -e

if [[ "$CLIENT_STATUS" != "0" ]]; then
    fail "client exited with status $CLIENT_STATUS"
fi

for _ in {1..80}; do
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        DAEMON_PID=""
        break
    fi

    sleep 0.1
done

if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon did not stop after client disconnected"
fi

grep -q "injecting mock transfer status" "$DAEMON_LOG" \
    || fail "daemon did not inject transfer statuses"
grep -q "transfer_status=Timeout" "$DAEMON_LOG" \
    || fail "daemon did not inject timeout"
grep -q "transfer_status=Stall" "$DAEMON_LOG" \
    || fail "daemon did not inject stall"
grep -q "injecting mock short packet" "$DAEMON_LOG" \
    || fail "daemon did not inject short packet"
grep -q "received network frame request_id=10 frame_type=DetachRequest" "$DAEMON_LOG" \
    || fail "daemon did not receive detach after fault responses"
grep -q "endpoint transfer queues clean at session cleanup" "$DAEMON_LOG" \
    || fail "daemon did not clean endpoint transfer queues"
grep -q "transfer_status=Timeout" "$CLIENT_LOG" \
    || fail "client did not observe timeout response"
grep -q "transfer_status=Stall" "$CLIENT_LOG" \
    || fail "client did not observe stall response"
grep -q "received network frame request_id=9 frame_type=Response status=0 payload_len=4" "$CLIENT_LOG" \
    || fail "client did not receive short packet response"
grep -q "mock VHCI processed URB urb_len=4" "$CLIENT_LOG" \
    || fail "client did not inject short packet into mock VHCI"
grep -q "received network frame request_id=10 frame_type=DetachResponse" "$CLIENT_LOG" \
    || fail "client did not receive detach response"

echo "local mock fault test passed"
echo "logs: $LOG_DIR"
