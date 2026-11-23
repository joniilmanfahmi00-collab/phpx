#!/usr/bin/env bash
# Isolated MySQL for WordPress smoke tests (local dev only; CI uses a service).
# Usage: scripts/mysql-smoke.sh [start|stop]
# Env: PHPX_MYSQLD (mysqld binary, default `mysqld` on PATH),
#      PHPX_MYSQL_DATADIR (default data/smoke/mysql, resolved absolute),
#      PHPX_WP_DB_PORT (default 13306), PHPX_WP_DB_NAME/USER/PASS (wordpress/wp/wp).
set -euo pipefail
MYSQLD="${PHPX_MYSQLD:-mysqld}"
DATADIR_REL="${PHPX_MYSQL_DATADIR:-data/smoke/mysql}"
case "$DATADIR_REL" in
  /*) DATADIR="$DATADIR_REL";;
  *) DATADIR="$(pwd)/$DATADIR_REL";;
esac
PORT="${PHPX_WP_DB_PORT:-13306}"
DB="${PHPX_WP_DB_NAME:-wordpress}"
USER="${PHPX_WP_DB_USER:-wp}"
PASS="${PHPX_WP_DB_PASS:-wp}"
PIDFILE="$DATADIR/phpx-mysqld.pid"

cmd="${1:-start}"
if [ "$cmd" = "stop" ]; then
  if [ -f "$PIDFILE" ]; then
    kill "$(cat "$PIDFILE")" 2>/dev/null || true
    rm -f "$PIDFILE"
    echo "[mysql-smoke] stopped"
  else
    echo "[mysql-smoke] not running (no pid file)"
  fi
  exit 0
fi

if mysqladmin --protocol=TCP -h 127.0.0.1 -P "$PORT" -u root ping 2>/dev/null; then
  echo "[mysql-smoke] already up on 127.0.0.1:$PORT"
else
  command -v "$MYSQLD" >/dev/null || { echo "mysqld not found (set PHPX_MYSQLD)"; exit 2; }
  if [ ! -f "$DATADIR/auto.cnf" ]; then
    echo "[mysql-smoke] initializing datadir $DATADIR..."
    "$MYSQLD" --initialize-insecure --datadir="$DATADIR"
  fi
  echo "[mysql-smoke] starting mysqld on 127.0.0.1:$PORT..."
  "$MYSQLD" --datadir="$DATADIR" --port="$PORT" --bind-address=127.0.0.1 --pid-file="$PIDFILE" &
  for i in $(seq 1 30); do
    mysqladmin --protocol=TCP -h 127.0.0.1 -P "$PORT" -u root ping 2>/dev/null && break
    sleep 1
  done
  mysqladmin --protocol=TCP -h 127.0.0.1 -P "$PORT" -u root ping || { echo "mysqld failed to start"; exit 1; }
fi
mysql --protocol=TCP -h 127.0.0.1 -P "$PORT" -u root -e \
  "CREATE DATABASE IF NOT EXISTS \`$DB\`; CREATE USER IF NOT EXISTS '$USER'@'%' IDENTIFIED BY '$PASS'; GRANT ALL PRIVILEGES ON \`$DB\`.* TO '$USER'@'%'; FLUSH PRIVILEGES;"
echo "[mysql-smoke] ready: 127.0.0.1:$PORT db=$DB user=$USER"
