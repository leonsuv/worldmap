#!/usr/bin/env bash
set -euo pipefail

# WorldMap Infrastructure Explorer — development servers
# Starts Rust backend + Vite frontend in parallel.
# Ctrl-C stops both.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Trap to kill both child processes on exit
cleanup() {
    echo ""
    echo "Shutting down…"
    kill 0 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT INT TERM

# ---------- Backend -----------------------------------------------------------

echo "Starting backend (cargo run)…"
(
    cd "$ROOT_DIR/backend"
    cargo run 2>&1 | sed 's/^/[backend] /'
) &
BACKEND_PID=$!

# Give the backend a moment to start compiling
sleep 1

# ---------- Frontend ----------------------------------------------------------

echo "Starting frontend (vite dev)…"
(
    cd "$ROOT_DIR/frontend"
    npm run dev 2>&1 | sed 's/^/[frontend] /'
) &
FRONTEND_PID=$!

echo ""
echo "Backend PID:  $BACKEND_PID"
echo "Frontend PID: $FRONTEND_PID"
echo "Press Ctrl-C to stop both."
echo ""

# Wait for either to exit
wait
