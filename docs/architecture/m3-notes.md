# Catatan M3 PHPX - pratinjau developer v0.1

Tujuan: server aplikasi PHP minimal yang berguna. Status: SELESAI (pratinjau developer, bukan SLA produksi).

## Yang berubah vs M2

- CLI difinalkan ke permukaan SPEC 9:
  `serve [--host --port]`, `doctor`, `version`, `runtime list|inspect`, `workers status|reload`,
  `queue status|work` (stub ditunda M5, exit 2), `scheduler run` (stub ditunda M5, exit 2), `metrics` (catatan dev-preview).
  Resolusi `--config` global: `--config` > `./phpx.toml` > `phpx.toml.example`. `--json` global bila didukung.
  Kode exit: 0 ok, 1 runtime, 2 config/penggunaan/deferred. Lihat docs/operations/configuration.md.
- Gateway: ditambah `GET /healthz` (liveness, selalu 200 saat berjalan) + `GET /readyz` (readiness, cek docroot + entrypoint).
  Tanpa dispatch PHP, tanpa baca body untuk path health. Mencakup health check dasar SPEC 11 untuk M3.
- Config: `phpx.toml.example` ditulis ulang dengan default terdokumentasi + panduan Docker/systemd. `PhpxConfig` menurunkan `Serialize` untuk output status JSON.
- Pengemasan: `Dockerfile` (multi-stage rust:1.99.1-bookworm -> php:8.4-cli-bookworm, non-root `phpx` uid 10001, `HEALTHCHECK /healthz`, default `/app/phpx.toml` menyajikan fixture vanilla).
  `.dockerignore`, `packaging/systemd/phpx.service` (User=phpx, drain SIGINT graceful, flag hardening).
- Dokumen: `operations/configuration.md`, `quickstart-linux.md`, `quickstart-docker.md`, `php-version-matrix.md`, `limitations.md`, `systemd.md`.
  README dilabeli pratinjau developer. Kotak roadmap M3 dicentang.
- Pengujian: suite M2 tetap hijau. `tests/m3_cli.rs` baru (kontrak CLI: kode exit version/doctor/runtime/workers/queue/scheduler/metrics + JSON) dan cakupan health di `tests/m3_gateway.rs` (`/healthz` + `/readyz` 200 + x-request-id).
  Skrip: `scripts/smoke-linux.sh`, `scripts/smoke-docker.sh`, `scripts/bench.sh` (baseline reprodusibel, fallback wrk/curl, BUKAN klaim performa).

## Bukti keluar

- `cargo fmt --check` bersih, `cargo clippy --workspace --all-targets -- -D warnings` bersih.
- `cargo test --workspace` hijau (M1 5+2, M2 3+2, M3 CLI + gateway health).
- `phpx doctor` M3 mencetak cek config + petunjuk langkah berikutnya.
- Quickstart Docker: `docker build -t phpx:0.1.0-dev .` + `docker run -p 8080:8080` + `/healthz` + `/` + statis lolos; `whoami` -> `phpx`.
- Quickstart Linux native: `cargo build -p phpx-cli`, `phpx doctor`, `phpx serve --config phpx.toml`, `/healthz` + `/readyz` + `/` + statis + traversal 400 lolos.
- Fixture vanilla lolos (front-controller + statis + tanpa bocor source, sama seperti M2).
- Limitasi didokumentasikan; v0.1 dilabeli pratinjau developer di mana-mana (bantuan CLI, banner serve, README, limitations).

## Ditunda (eksplisit, tidak boleh memblokir M3)

- Backend queue/scheduler/cache/storage -> M5. Stub CLI exit 2 dengan catatan ditunda.
- Metrik Prometheus, counter live per-worker, reload SIGHUP, uji soak -> M4.
- Runtime embedded -> M6 eksperimental.
- HTTP/2 -> gated, tidak diklaim. HTTP/3, CDN, WAF, paritas reverse-proxy -> ditunda per SPEC 6.2.
- Fixture framework (Composer/Laravel/WordPress) -> M4.

Ketertelusuran: SPEC 15 butir 1-4, 8-10 tercakup oleh M1-M3; butir 5-7 + 11-12 selesai di M4.
