#!/usr/bin/env bash
set -euo pipefail

ADDR="${WHY_USB_INTEGRATION_ADDR:-127.0.0.1:3015}"
CLIENT_TIMEOUT_SECONDS="${WHY_USB_CLIENT_TIMEOUT_SECONDS:-12s}"
LOG_DIR="${WHY_USB_INTEGRATION_LOG_DIR:-$(mktemp -d)}"
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
    echo "local mock integration failed: $*" >&2
    echo "--- daemon log: $DAEMON_LOG ---" >&2
    if [[ -f "$DAEMON_LOG" ]]; then
        sed -n '1,240p' "$DAEMON_LOG" >&2
    fi
    echo "--- client log: $CLIENT_LOG ---" >&2
    if [[ -f "$CLIENT_LOG" ]]; then
        sed -n '1,240p' "$CLIENT_LOG" >&2
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
    WHY_USB_MOCK_FRAME_LIMIT=2 WHY_USB_MOCK_FRAME_INTERVAL_MS=100 \
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

grep -q "received network frame request_id=1 frame_type=AttachRequest" "$DAEMON_LOG" \
    || fail "daemon did not receive attach request"
grep -q "protocol session attached" "$DAEMON_LOG" \
    || fail "daemon did not attach protocol session"
grep -q "sending attach descriptors" "$DAEMON_LOG" \
    || fail "daemon did not send attach descriptors"
grep -q "handled GET_DESCRIPTOR" "$DAEMON_LOG" \
    || fail "daemon did not handle GET_DESCRIPTOR"
grep -q "handled SET_ADDRESS" "$DAEMON_LOG" \
    || fail "daemon did not handle SET_ADDRESS"
grep -q "handled SET_CONFIGURATION" "$DAEMON_LOG" \
    || fail "daemon did not handle SET_CONFIGURATION"
grep -q "queued mock HID input reports" "$DAEMON_LOG" \
    || fail "daemon did not queue HID input reports"
grep -q "events=4" "$DAEMON_LOG" \
    || fail "daemon did not queue the configured HID input report sequence"
grep -q "received network frame request_id=7 frame_type=Request" "$DAEMON_LOG" \
    || fail "daemon did not receive first data request"
grep -q "dispatching endpoint transfer" "$DAEMON_LOG" \
    || fail "daemon did not dispatch endpoint transfer through queue"
grep -q "received network frame request_id=9 frame_type=DetachRequest" "$DAEMON_LOG" \
    || fail "daemon did not receive detach request"
grep -q "device detached through driver control plane" "$DAEMON_LOG" \
    || fail "daemon did not detach mock device"
grep -q "endpoint transfer queues clean at session cleanup" "$DAEMON_LOG" \
    || fail "daemon did not clean endpoint transfer queues"
grep -q "received lifecycle frame request_id=1 frame_type=AttachResponse" "$CLIENT_LOG" \
    || fail "client did not receive attach response"
grep -q "received attach descriptors" "$CLIENT_LOG" \
    || fail "client did not decode attach descriptors"
grep -q "mock USB enumeration completed" "$CLIENT_LOG" \
    || fail "client did not complete mock enumeration"
grep -q "mock HID report descriptor validated" "$CLIENT_LOG" \
    || fail "client did not validate HID report descriptor"
grep -q "mock HID keyboard input report" "$CLIENT_LOG" \
    || fail "client did not decode HID keyboard input report"
grep -q "keycodes=\\[4, 0, 0, 0, 0, 0\\]" "$CLIENT_LOG" \
    || fail "client did not decode configured HID key A"
grep -q "keycodes=\\[40, 0, 0, 0, 0, 0\\]" "$CLIENT_LOG" \
    || fail "client did not decode configured HID Enter key"
grep -q "received network frame request_id=7 frame_type=Response" "$CLIENT_LOG" \
    || fail "client did not receive data response"
grep -q "received network frame request_id=9 frame_type=DetachResponse" "$CLIENT_LOG" \
    || fail "client did not receive detach response"
grep -q "mock VHCI processed URB" "$CLIENT_LOG" \
    || fail "client did not process returned URB"

echo "local mock integration passed"
echo "logs: $LOG_DIR"
