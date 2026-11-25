# Cache PHPX (M5 fase 3 + susulan Redis)

Status: fase 3 SELESAI — memori + filesystem + Redis terbatas, TTL, uji kontrak.
Redis app-native via REDIS_HOST WordPress/Laravel tetap jalan langsung dari PHP, tanpa driver PHPX.

SPEC 7.3: OPcache PHP tetap milik PHP. Cache ini hanya data aplikasi
(string/byte), tidak pernah opcode. Ketiga driver terbatas dan berbagi
satu suite kontrak.

## Driver

- `memory`: HashMap + eviksi FIFO, per-proses, ephemeral.
  Hit/miss dihitung in-process. Bagus untuk dev + pengujian.
- `filesystem`: satu file per kunci (`<key>.cache`, 8-byte BE expiry + value),
  eviksi oldest-mtime, tahan restart. Bagus untuk baseline satu-node.
- `redis`: driver bersama/berjaringan (suite kontrak sama + pin TTL-expiry + isolasi
  namespace). Server di `[cache] url` atau env `PHPX_REDIS_URL`. Envelope
  8-byte-BE-expiry yang sama seperti filesystem. Eviksi oldest-first via indeks sorted-set;
  byte dihitung di kunci counter (drift kedaluwarsa-lazy dimungkinkan, sekelas
  drift mtime filesystem). Hit/miss adalah counter lokal-proses. Kunci di-namespace
  (`phpx:cache:<namespace>:`; CLI memakai `default`). Secret: URL
  tidak pernah di-log.

## Semantik (SPEC 7.3)

- Terbatas: kunci 1-256 char `[A-Za-z0-9 . _ - :]`, value maks 1MB,
  entri maks 100.000, byte maks 1GB. Penuh = error (backpressure, tidak pernah tanpa batas).
- TTL: `set --ttl-secs N`, `0` = tanpa kedaluwarsa, maks 30 hari. Kedaluwarsa = miss + hapus-lazy.
- Operasi: `get` / `set` / `delete` / `clear` / `stats`.
- Exactly-once TIDAK dijanjikan untuk pemanggil; cache bersifat best-effort + TTL.
  App wajib toleran-miss (hitung-ulang) — sama seperti miss OPcache.

## CLI (fase 3 nyata)

- `phpx cache status [--json]` — entri, hit/miss, batas.
- `phpx cache get --key foo [--json]` — hit mencetak value, miss exit 0 + catatan.
- `phpx cache set --key foo --value bar [--ttl-secs 60] [--json]`
- `phpx cache delete --key foo [--json]` — miss saat delete = exit 0 + catatan.
- `phpx cache clear [--json]` — mengembalikan jumlah yang dihapus.
- Kunci/value buruk = exit 2 dengan pesan actionable.

## Config

```toml
[cache]
driver = "memory" # memory | filesystem | redis
max_memory = "128MB" # 1KB-1GB, diurai oleh parse_size
dir = "./data/cache" # wajib bila driver=filesystem
url = "redis://127.0.0.1:6379" # wajib bila driver=redis (atau env PHPX_REDIS_URL)
max_entries = 1000 # 1-100000
```

Validasi (`phpx-config`): driver tak dikenal exit 2, filesystem tanpa
`dir` exit 2, redis tanpa `url` (dan tanpa `PHPX_REDIS_URL`) exit 2,
`max_memory` wajib terurai 1KB-1GB, `max_entries` 1-100000.

## Pengujian

- `crates/phpx-cache/tests/contract.rs` (suite yang sama vs semua driver):
  set/get, TTL/delete/clear, tahan-dibuka-ulang (hanya filesystem), kedaluwarsa TTL
  (redis), isolasi namespace (redis), menolak kunci buruk. Kasus Redis berjalan
  melawan `PHPX_REDIS_URL` live dan SKIP dengan lantang tanpanya.
- `cargo test -p phpx-cache` hijau (2 unit + 10 kontrak).
- CLI: `cache status` exit 0, `--json` punya `entries`.

## Ditunda (M5 lanjutan)

- Middleware cache terintegrasi gateway (butuh integrasi runtime-process).
- Pemanasan cache / proteksi stampede.
