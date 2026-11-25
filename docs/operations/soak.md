# Soak + analisis pertumbuhan memori (M4)

Worker one-shot mengisolasi state secara konstruksi, jadi soak di sini mencari
regresi gateway/pool (pertumbuhan RSS, badai restart, drift latensi), bukan
kebocoran PHP di dalam worker (setiap proses PHP keluar per request).

## Soak cepat (Linux/macOS/WSL2)

```bash
./scripts/soak.sh --port 18081 --requests 2000 --concurrency 8
```

Fungsinya: membangun `phpx` (debug), melayani `examples/vanilla-php`, me-loop
curl `/`, `/hello.php`, `/style.css`, `/metrics`, mengambil sampel RSS gateway
(`ps -o rss=`), mengikis counter `/metrics`, dan mencetak laporan restart/latensi.

## Soak panjang (skrip)

- Linux/WSL: `scripts/soak-long.sh [--minutes 60] [--mode one-shot|persistent] [--profile debug|release] [--out-dir DIR] [--port 18081]`
- Windows: `scripts/soak-long-windows.ps1 [-Minutes 60] [-Concurrency 8] [-Port 18081] [-Mode one-shot|persistent] [-Profile release] [-OutDir ...]`

Keduanya menulis `phpx-soaklong.toml` (konfig persis), `phpx-soaklong.log`,
`phpx-soaklong-rss.csv` (satu sampel RSS per ~60 dtk), dan snapshot
`phpx-soaklong.out.metrics` ke direktori output.

## Yang dicatat (ADR-0014 Seksi 5)

- Tren RSS gateway (awal/akhir/maks) + slope (KB/menit). RSS PHP one-shot itu
  per-request dan direklamasi OS; RSS gateway harus flat.
- Counter `/metrics`: `completed`, `failed`, `timeouts`, `rejected_overload`,
  `recycled`, `restarts`, `circuit_breaks`; plus puncak `active`/`waiting`.
- Jumlah restart + alasan (5xx vs timeout vs sirkuit), throughput (req/dtk),
  latensi p50/p95 dari laporan skrip.
- Penskalaan adaptif tetap NONAKTIF di M4 (baseline pool tetap per ADR-0014 Fase D).

## Ambang kelulusan (M4)

- Tidak ada kontaminasi silang (`isolate.php` tetap `clean`).
- Jalur crash + drain + 503 dilatih tanpa restart gateway.
- Slope RSS ~flat; tidak ada pertumbuhan tak terbatas; sirkuit hanya terbuka
  pada kegagalan injeksi (mis. loop `crash.php`), lalu pulih.
- Laporan masuk ke catatan rilis; log mentah disimpan 30 hari.

## Rekaman run (bukti arsip)

### 2026-11-25: 6 jam, persistent/release, Windows nativ (+)

- 73.600 / 73.600 request (gagal 0, timeout 0, ditolak 0, sirkuit 0).
- RSS gateway 17.744 -> 17.436 KB (min 17.424, maks 18.004, rata-rata 17.785);
  slope **-0,86 KB/menit** - flat, tidak ada sinyal bocor.
- 147 restart, semuanya recycle-budget (73600/500 = 147,2, cocok persis).
- Arsip: `data/bench/soak-6h-persistent-release-win/` (`SUMMARY.md`, CSV,
  metrics, log, toml). Direktori `data/` di-gitignore; ringkasannya tercatat
  di sini agar tertelusur.

### 2026-11-23: 60 menit, persistent/release, Windows (+)

- 12.600 / 12.600 (gagal 0, timeout 0, ditolak 0, sirkuit 0).
- RSS 8732 -> 8948 KB (min 8704, maks 9028); slope +3,6 KB/menit.
- 25 restart recycle-budget (12600/500 = 25,2, cocok persis).
- Arsip: `data/bench/soak-long-persistent-release/` + `SUMMARY.md`.

### 2026-11-25: 50 menit parsial, persistent/release, WSL (interim)

- Terhenti dini oleh kesalahan operator (gateway ter-kill `pkill` saat
  diagnosis lain); 58 sampel valid: RSS 9488 -> 9944 KB.
- Arsip: `data/bench/soak-6h-persistent-release/`. Bukan pengganti run 6 jam.

(+) Satu node, mesin developer - bukan SLA, bukan klaim armada.
