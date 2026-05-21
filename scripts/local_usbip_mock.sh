#!/usr/bin/env bash
set -euo pipefail

ADDR="${WHY_USB_USBIP_ADDR:-127.0.0.1:3025}"
CLIENT_TIMEOUT_SECONDS="${WHY_USB_CLIENT_TIMEOUT_SECONDS:-12s}"
LOG_DIR="${WHY_USB_USBIP_LOG_DIR:-$(mktemp -d)}"
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
    echo "local USB/IP mock integration failed: $*" >&2
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

cargo build -p daemon

env RUST_LOG=info WHY_USB_DAEMON_PROTOCOL=usbip WHY_USB_ATTACH_DEVICE=1234:5678:1:2 \
    WHY_USB_MOCK_HID_KEYS=a target/debug/daemon "$ADDR" >"$DAEMON_LOG" 2>&1 &
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
timeout "$CLIENT_TIMEOUT_SECONDS" python3 - "$ADDR" >"$CLIENT_LOG" 2>&1 <<'PY'
import socket
import struct
import sys

CMD_SUBMIT = 1
CMD_UNLINK = 2
RET_SUBMIT = 3
RET_UNLINK = 4
USBIP_DIR_OUT = 0
USBIP_DIR_IN = 1
NO_ISO_PACKETS = 0xFFFFFFFF

addr = sys.argv[1]
host, port = addr.rsplit(":", 1)


def setup(request_type, request, value, index, length):
    return struct.pack("<BBHHH", request_type, request, value, index, length)


def submit(seqnum, direction, endpoint, transfer_len, setup_bytes, payload=b""):
    header = struct.pack(
        ">IIIII",
        CMD_SUBMIT,
        seqnum,
        0x00010002,
        direction,
        endpoint,
    )
    body = struct.pack(
        ">IIIII",
        0,
        transfer_len,
        0,
        NO_ISO_PACKETS,
        0,
    )
    packet = header + body + setup_bytes + payload
    sock.sendall(packet)
    return read_ret_submit(seqnum)


def unlink(seqnum, unlink_seqnum):
    packet = struct.pack(
        ">IIIIII",
        CMD_UNLINK,
        seqnum,
        0x00010002,
        USBIP_DIR_OUT,
        0,
        unlink_seqnum,
    ) + bytes(24)
    sock.sendall(packet)
    header = read_exact(48)
    command, ret_seqnum, _devid, _direction, _endpoint = struct.unpack(">IIIII", header[:20])
    status = struct.unpack(">i", header[20:24])[0]
    assert command == RET_UNLINK, command
    assert ret_seqnum == seqnum, ret_seqnum
    assert status == 0, status


def read_exact(length):
    data = b""
    while len(data) < length:
        chunk = sock.recv(length - len(data))
        if not chunk:
            raise RuntimeError("socket closed")
        data += chunk
    return data


def read_ret_submit(expected_seqnum):
    header = read_exact(48)
    command, seqnum, _devid, _direction, _endpoint = struct.unpack(">IIIII", header[:20])
    status = struct.unpack(">i", header[20:24])[0]
    actual_length = struct.unpack(">I", header[24:28])[0]
    payload = read_exact(actual_length) if actual_length else b""
    assert command == RET_SUBMIT, command
    assert seqnum == expected_seqnum, seqnum
    return status, payload


with socket.create_connection((host, int(port)), timeout=5) as sock:
    status, device = submit(
        1,
        USBIP_DIR_IN,
        0,
        18,
        setup(0x80, 0x06, 0x0100, 0, 18),
    )
    assert status == 0, status
    assert len(device) == 18, len(device)
    assert device[0:2] == b"\x12\x01", device[:2]
    assert device[8:12] == bytes([0x34, 0x12, 0x78, 0x56]), device[8:12]

    status, config = submit(
        2,
        USBIP_DIR_IN,
        0,
        255,
        setup(0x80, 0x06, 0x0200, 0, 255),
    )
    assert status == 0, status
    assert config[0:2] == b"\x09\x02", config[:2]

    status, report_desc = submit(
        3,
        USBIP_DIR_IN,
        0,
        63,
        setup(0x81, 0x06, 0x2200, 0, 63),
    )
    assert status == 0, status
    assert len(report_desc) == 63, len(report_desc)

    status, payload = submit(
        4,
        USBIP_DIR_OUT,
        0,
        0,
        setup(0x00, 0x05, 1, 0, 0),
    )
    assert status == 0, status
    assert payload == b"", payload

    status, payload = submit(
        5,
        USBIP_DIR_OUT,
        0,
        0,
        setup(0x00, 0x09, 1, 0, 0),
    )
    assert status == 0, status
    assert payload == b"", payload

    status, report = submit(6, USBIP_DIR_IN, 0x81, 8, bytes(8))
    assert status == 0, status
    assert report == b"\x00\x00\x04\x00\x00\x00\x00\x00", report

    status, report = submit(7, USBIP_DIR_IN, 0x81, 8, bytes(8))
    assert status == 0, status
    assert report == b"\x00\x00\x00\x00\x00\x00\x00\x00", report

    unlink(8, 7)

print("USB/IP mock client passed")
PY
CLIENT_STATUS="$?"
set -e

if [[ "$CLIENT_STATUS" != "0" ]]; then
    fail "USB/IP client exited with status $CLIENT_STATUS"
fi

for _ in {1..80}; do
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        DAEMON_PID=""
        break
    fi

    sleep 0.1
done

if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon did not stop after USB/IP client disconnected"
fi

grep -q "USB/IP mock session attached" "$DAEMON_LOG" \
    || fail "daemon did not attach USB/IP mock session"
grep -q "handled GET_DESCRIPTOR" "$DAEMON_LOG" \
    || fail "daemon did not handle USB/IP GET_DESCRIPTOR"
grep -q "handled SET_ADDRESS" "$DAEMON_LOG" \
    || fail "daemon did not handle USB/IP SET_ADDRESS"
grep -q "handled SET_CONFIGURATION" "$DAEMON_LOG" \
    || fail "daemon did not handle USB/IP SET_CONFIGURATION"
grep -q "handled USB/IP HID interrupt IN submit" "$DAEMON_LOG" \
    || fail "daemon did not handle USB/IP HID interrupt IN"
grep -q "USB/IP mock session closed" "$DAEMON_LOG" \
    || fail "daemon did not close USB/IP mock session"
grep -q "USB/IP mock client passed" "$CLIENT_LOG" \
    || fail "USB/IP client did not complete assertions"

echo "local USB/IP mock integration passed"
echo "logs: $LOG_DIR"
