# PHPX upgrade + rollback (M7)

## Versioning

- SemVer, config `schema_version = 1`. Minor/patch keep v1 parsing (new fields have serde defaults).
- `phpx doctor` warns on PHP target mismatch; `phpx workers status` shows active pool bounds.

## Upgrade (stateless gateway, 1-2 min)

1. Snapshot durable state (see backup-recovery.md): `data/queue.db`, `data/scheduler.db`, `data/cache/`, `data/storage/`, `phpx.toml`.
2. Pull new image/binary, validate: `phpx doctor`, `phpx workers status`, `curl /readyz`.
3. Drain + restart:
   ```bash
   systemctl stop phpx        # graceful drain bounded by shutdown_drain_timeout_seconds
   # replace /usr/local/bin/phpx or image tag
   systemctl start phpx && curl -fsS http://127.0.0.1:8080/readyz
   ```
   Docker: `docker stop --time=30 phpx && docker run ... new-tag`.
4. Watch `/metrics` 5 min: `restarts` flat, `rejected_overload` ~0, p95 stable vs bench baseline.

## Rollback

- Same binary/image previous tag + previous `phpx.toml` (kept in step 1). No DB migration in v0.1 (SQLite schema additive only), so rollback = restart old version.
- If queue DB was written by newer version, old binary still reads it (additive schema); if unsure, restore `queue.db` snapshot then restart.

## systemd resource example

```ini
[Service]
MemoryMax=1G
CPUQuota=200%
TasksMax=256
```
Tune per `bench.sh` baseline; saturation must 503 (observable), never OOM-loop.
