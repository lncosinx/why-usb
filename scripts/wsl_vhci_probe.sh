#!/usr/bin/env bash
set -euo pipefail

LOG_DIR="${WHY_USB_WSL_PROBE_LOG_DIR:-$(mktemp -d)}"
CLIENT_LOG="$LOG_DIR/client.log"

mkdir -p "$LOG_DIR"

cargo build -p client

set +e
env RUST_LOG=info WHY_USB_VHCI_BACKEND=linux WHY_USB_VHCI_PROBE_ONLY=1 \
    target/debug/client >"$CLIENT_LOG" 2>&1
CLIENT_STATUS="$?"
set -e

if [[ "$CLIENT_STATUS" == "0" ]]; then
    echo "WSL/Linux vhci_hcd probe passed"
else
    echo "WSL/Linux vhci_hcd probe did not pass in this environment"
fi

echo "--- client probe log ---"
sed -n '1,200p' "$CLIENT_LOG"
echo "logs: $LOG_DIR"
