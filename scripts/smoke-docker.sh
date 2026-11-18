#!/usr/bin/env bash
# PHPX M3 smoke test - Docker (CI + local).
set -euo pipefail
IMG="${PHPX_IMAGE:-phpx:0.1.0-dev}"
PORT="${PHPX_DOCKER_PORT:-18081}"
NAME="phpx-smoke-$PORT"

echo "[docker-smoke] building $IMG..."
docker build -t "$IMG" .

echo "[docker-smoke] non-root check..."
docker run --rm "$IMG" whoami | grep -qx phpx

echo "[docker-smoke] running detached on $PORT..."
docker run -d --rm --name "$NAME" -p "$PORT:8080" "$IMG" >/dev/null
trap "docker stop $NAME >/dev/null 2>&1 || true" EXIT
for i in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -fsS "http://127.0.0.1:$PORT/healthz" | grep -q ok
curl -fsS "http://127.0.0.1:$PORT/readyz" | grep -q ready
curl -fsS "http://127.0.0.1:$PORT/" >/dev/null
curl -fsS "http://127.0.0.1:$PORT/style.css" >/dev/null
code=$(curl -o /dev/null -s -w "%{http_code}" "http://127.0.0.1:$PORT/%2e%2e/x" || true)
if [ "$code" != "400" ]; then echo "FAIL: traversal expected 400 got $code"; docker logs "$NAME" | tail -20; exit 1; fi

echo "[docker-smoke] ALL GREEN ($IMG as non-root)"
docker stop "$NAME" >/dev/null
trap - EXIT
