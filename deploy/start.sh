#!/bin/sh
# Starts PocketBase on localhost, waits until it is healthy, then runs the app.
# If either process exits, the container stops so the platform restarts it.
set -eu

PB_DATA="${PB_DATA:-/data/pb_data}"
PB_MIGRATIONS="${PB_MIGRATIONS:-/app/pb_migrations}"
PB_BIN="${PB_BIN:-pocketbase}"
# Localhost only by default. Set PB_HTTP=0.0.0.0:8090 temporarily to reach the PocketBase dashboard.
PB_HTTP="${PB_HTTP:-127.0.0.1:8090}"
APP_BIN="${APP_BIN:-/app/adire-house}"
mkdir -p "$PB_DATA"

"$PB_BIN" serve --http="$PB_HTTP" --dir="$PB_DATA" --migrationsDir="$PB_MIGRATIONS" &
PB_PID=$!

i=0
until curl -fsS http://127.0.0.1:8090/api/health >/dev/null 2>&1; do
  i=$((i + 1))
  if [ "$i" -gt 60 ] || ! kill -0 "$PB_PID" 2>/dev/null; then
    echo "PocketBase failed to start" >&2
    exit 1
  fi
  sleep 1
done
echo "PocketBase is up"

"$APP_BIN" &
APP_PID=$!

trap 'kill "$PB_PID" "$APP_PID" 2>/dev/null' TERM INT

# Exit as soon as either process stops.
while kill -0 "$PB_PID" 2>/dev/null && kill -0 "$APP_PID" 2>/dev/null; do
  sleep 2
done
echo "A process exited; stopping" >&2
kill "$PB_PID" "$APP_PID" 2>/dev/null || true
wait || true
exit 1
