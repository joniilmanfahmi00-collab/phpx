# PHPX backup + recovery (M7, durable queue)

## What is durable

| Path (defaults) | Content | Backup |
|-----------------|---------|--------|
| `data/queue.db` (sqlite) | durable queue jobs + dead-letter | file snapshot (stop writer or SQLite backup API) |
| `data/scheduler.db` | scheduler cursor/history | same as queue |
| `data/cache/` (filesystem driver) | cache entries | file copy (memory driver = ephemeral, no backup) |
| `data/storage/` (local driver) | app objects | file copy / rsync |
| `phpx.toml` | operator config | version control |

Memory drivers (`queue` memory, `cache` memory) are ephemeral by design: no backup, documented in `queue.md`/`cache.md`.

## Backup procedure (single-node baseline)

```bash
# Consistent SQLite snapshot (libsqlite backup; no live-write corruption)
sqlite3 data/queue.db ".backup 'backup/queue-$(date +%F).db'"
sqlite3 data/scheduler.db ".backup 'backup/scheduler-$(date +%F).db'"
rsync -a data/storage/ backup/storage/
cp phpx.toml backup/phpx.toml
```

## Recovery

1. Stop writer: `systemctl stop phpx` (or stop container).
2. Restore files to `data/` + config, fix ownership (`chown phpx:phpx`).
3. Start + verify: `phpx queue status`, `phpx scheduler run --dry-run` (where supported), `curl /readyz`, `phpx metrics`.
4. RPO/RTO are operator-defined (cron frequency); v0.1 has no PITR, no encrypted-at-rest (file perms + disk encryption are operator responsibility).

## Integrity check

```bash
sqlite3 data/queue.db "PRAGMA integrity_check;"
phpx queue status --json
```
