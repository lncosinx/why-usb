#!/usr/bin/env bash
set -euo pipefail

ITERATIONS="${WHY_USB_STRESS_ITERATIONS:-5}"
BASE_PORT="${WHY_USB_STRESS_BASE_PORT:-3110}"
LOG_DIR="${WHY_USB_STRESS_LOG_DIR:-$(mktemp -d)}"

mkdir -p "$LOG_DIR"

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]] || [[ "$ITERATIONS" -lt 1 ]]; then
    echo "WHY_USB_STRESS_ITERATIONS must be a positive integer" >&2
    exit 1
fi

if ! [[ "$BASE_PORT" =~ ^[0-9]+$ ]] || [[ "$BASE_PORT" -lt 1 ]] || [[ "$BASE_PORT" -gt 65535 ]]; then
    echo "WHY_USB_STRESS_BASE_PORT must be a TCP port number" >&2
    exit 1
fi

LAST_PORT=$((BASE_PORT + ITERATIONS - 1))
if [[ "$LAST_PORT" -gt 65535 ]]; then
    echo "stress port range exceeds 65535: $BASE_PORT..$LAST_PORT" >&2
    exit 1
fi

for iteration in $(seq 1 "$ITERATIONS"); do
    port=$((BASE_PORT + iteration - 1))
    addr="127.0.0.1:$port"
    iteration_log_dir="$LOG_DIR/iteration-$iteration"

    mkdir -p "$iteration_log_dir"
    echo "stress iteration $iteration/$ITERATIONS on $addr"

    WHY_USB_INTEGRATION_ADDR="$addr" \
    WHY_USB_INTEGRATION_LOG_DIR="$iteration_log_dir" \
        bash scripts/local_mock_integration.sh
done

echo "local mock attach/detach stress passed"
echo "iterations: $ITERATIONS"
echo "logs: $LOG_DIR"
