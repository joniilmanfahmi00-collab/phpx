# Upgrade + rollback PHPX (M7)

## Versi

- SemVer, config `schema_version = 1`. Minor/patch mempertahankan parsing v1 (field baru punya serde defaults).
- `phpx doctor` memperingatkan bila target PHP mismatch; `phpx workers status` menampilkan batas pool aktif.

## Upgrade (gateway stateless, 1-2 mnt)

1. Snapshot state tahan-lama (lihat backup-recovery.md): `data/queue.db`, `data/scheduler.db`, `data/cache/`, `data/storage/`, `phpx.toml`.
2. Tarik image/binary baru, validasi: `phpx doctor`, `phpx workers status`, `curl /readyz`.
3. Drain + restart:
   ```bash
   systemctl stop phpx        # graceful drain dibatasi oleh shutdown_drain_timeout_seconds
   # ganti /usr/local/bin/phpx atau tag image
   systemctl start phpx && curl -fsS http://127.0.0.1:8080/readyz
   ```
   Docker: `docker stop --time=30 phpx && docker run ... new-tag`.
4. Pantau `/metrics` 5 mnt: `restarts` datar, `rejected_overload` ~0, p95 stabil vs baseline bench.

## Rollback

- Binary/image tag sebelumnya yang sama + `phpx.toml` sebelumnya (disimpan di langkah 1). Tanpa migrasi DB di v0.1 (skema SQLite hanya aditif), jadi rollback = restart versi lama.
- Bila DB queue sudah ditulis oleh versi lebih baru, binary lama tetap bisa membacanya (skema aditif); bila ragu, restore snapshot `queue.db` lalu restart.

## Contoh resource systemd

```ini
[Service]
MemoryMax=1G
CPUQuota=200%
TasksMax=256
```
Tune per baseline `bench.sh`; saturasi wajib 503 (teramati), tidak boleh OOM-loop.
