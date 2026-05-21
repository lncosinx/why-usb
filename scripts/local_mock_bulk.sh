#!/usr/bin/env bash
set -euo pipefail

ADDR="${WHY_USB_BULK_ADDR:-127.0.0.1:3035}"
BULK_BYTES="${WHY_USB_BULK_BYTES:-4096}"
FRAME_LIMIT="${WHY_USB_BULK_FRAME_LIMIT:-3}"
CLIENT_TIMEOUT_SECONDS="${WHY_USB_CLIENT_TIMEOUT_SECONDS:-12s}"
LOG_DIR="${WHY_USB_BULK_LOG_DIR:-$(mktemp -d)}"
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
    echo "local mock bulk test failed: $*" >&2
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

env RUST_LOG=info WHY_USB_ATTACH_DEVICE=1234:5678:1:2 WHY_USB_MOCK_HID_KEYS=a,enter \
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
    WHY_USB_MOCK_FRAME_LIMIT="$FRAME_LIMIT" \
    WHY_USB_MOCK_FRAME_INTERVAL_MS=50 \
    WHY_USB_MOCK_BULK_BYTES="$BULK_BYTES" \
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

EXPECTED_URB_LEN=$((BULK_BYTES + 16))

grep -q "mock bulk transfer echoed" "$DAEMON_LOG" \
    || fail "daemon did not echo mock bulk transfer"
grep -q "bulk_len=$BULK_BYTES" "$DAEMON_LOG" \
    || fail "daemon did not observe expected bulk length"
grep -q "mock bulk transfer validated" "$CLIENT_LOG" \
    || fail "client did not validate mock bulk transfer"
grep -q "bulk_len=$BULK_BYTES" "$CLIENT_LOG" \
    || fail "client did not observe expected bulk length"
grep -q "mock VHCI processed URB urb_len=$EXPECTED_URB_LEN" "$CLIENT_LOG" \
    || fail "client did not inject expected bulk response length"
grep -q "received network frame request_id=$((7 + FRAME_LIMIT)) frame_type=DetachResponse" "$CLIENT_LOG" \
    || fail "client did not receive detach response after bulk workload"

echo "local mock bulk test passed"
echo "bulk bytes: $BULK_BYTES"
echo "frames: $FRAME_LIMIT"
echo "logs: $LOG_DIR"
