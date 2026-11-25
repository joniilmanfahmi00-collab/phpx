# Backup + pemulihan PHPX (M7, queue tahan-lama)

## Apa yang tahan-lama

| Path (default) | Isi | Backup |
|-----------------|---------|--------|
| `data/queue.db` (sqlite) | job queue tahan-lama + dead-letter | snapshot file (hentikan writer atau API backup SQLite) |
| `data/scheduler.db` | kursor/history scheduler | sama seperti queue |
| `data/cache/` (driver filesystem) | entri cache | salinan file (driver memory = ephemeral, tanpa backup) |
| `data/storage/` (driver local) | objek app | salinan file / rsync |
| `phpx.toml` | config operator | version control |

Driver memory (`queue` memory, `cache` memory) bersifat ephemeral by design: tanpa backup, terdokumentasi di `queue.md`/`cache.md`.

## Prosedur backup (baseline satu-node)

```bash
# Snapshot SQLite yang konsisten (backup libsqlite; tanpa korupsi tulis-live)
sqlite3 data/queue.db ".backup 'backup/queue-$(date +%F).db'"
sqlite3 data/scheduler.db ".backup 'backup/scheduler-$(date +%F).db'"
rsync -a data/storage/ backup/storage/
cp phpx.toml backup/phpx.toml
```

## Pemulihan

1. Hentikan writer: `systemctl stop phpx` (atau hentikan container).
2. Restore file ke `data/` + config, perbaiki kepemilikan (`chown phpx:phpx`).
3. Start + verifikasi: `phpx queue status`, `phpx scheduler run --dry-run` (bila didukung), `curl /readyz`, `phpx metrics`.
4. RPO/RTO ditentukan operator (frekuensi cron); v0.1 tanpa PITR, tanpa enkripsi-at-rest (perm file + enkripsi disk adalah tanggung jawab operator).

## Cek integritas

```bash
sqlite3 data/queue.db "PRAGMA integrity_check;"
phpx queue status --json
```
