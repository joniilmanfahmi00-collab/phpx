# Catatan implementasi M2 (gateway HTTP)

Status: terimplementasi, semua kriteria keluar terverifikasi pada PHP 8.3.30 / Rust 1.99.1 (Windows dev).

## Yang sudah masuk

- `crates/phpx-server`: listener Tokio + Hyper HTTP/1.1, host/port dari config.
- Routing: document-root + front-controller. `*.php` langsung selalu didispatch
  (tidak pernah disajikan mentah); file statis exact disajikan dengan tipe MIME; selebihnya
  diteruskan (fall through) ke entrypoint yang dikonfigurasi.
- Keamanan statis (SPEC 6.3): segmen `..`, backslash, NUL, dan `%2e`/`%2f`/`%5c` yang di-encode
  ditolak dengan 400 sebelum menyentuh FS; kandidat path dikanonikalisasi dan wajib berawalan
  docroot kanonis (escape symlink -> 403); `.php` tidak pernah disajikan secara statis.
- Dispatch PHP via `phpx-worker::BoundedPool` di dalam `spawn_blocking`
  (ADR-0006: PHP yang blocking tidak boleh di thread eksekutor Tokio).
- ID request (UUID v4) + header respons `x-request-id` + access log terstruktur
  (`request_id, method, path, status, duration_ms`).
- Batas body/header: `request_body_limit` diurai oleh `parse_size`
  ("16MB", "256M", "1GB", "512KB", byte polos); kelebihan -> 413.
- Backpressure: overload pool dipetakan ke 503 + `Retry-After: 1`, tidak pernah antre
  tanpa batas (SPEC 5.3).
- Crash worker (exit != 0) dipetakan ke 500 dengan diagnostik stderr; gateway
  tetap menyala dan request berikutnya berhasil.
- Shutdown graceful: Ctrl-C berhenti menerima; PHP in-flight dibatasi oleh
  execution timeout (drain).
- TLS via Rustls: `tls=true` mewajibkan `tls_cert_file` + `tls_key_file` (PEM),
  jika tidak maka error config yang jelas; HTTP polos untuk pengujian. HTTP/1.1 saja di M2;
  HTTP/2 ditunda (target SPEC 6.1, gated).

## Tambahan config (skema tetap v1)

- `[server] tls_cert_file`, `tls_key_file` (wajib bila `tls=true`).
- Helper `parse_size()` + `body_limit_bytes()` + `tls_enabled()`.

## Bukti kriteria keluar (`cargo test -p phpx-server`)

- `serves_front_controller_and_static`: `/` -> 200 fixture vanilla,
  `/style.css` -> 200 + content-type css, `/hello.php` -> 200 didispatch
  tanpa membocorkan source.
- `blocks_traversal_and_survives_crash`: path mentah `%2e`/`..%2f` -> 400,
  `/crash.php` -> 500 lalu `/` -> 200.
- `backpressure_returns_503_not_unbounded`: workers=1 + 6x konkuren sleep.php 5s
  -> minimal satu 503 dan satu 200.
- Unit: penolakan traversal + keamanan join docroot.
- Workspace: `cargo fmt --check` bersih,
  `cargo clippy --workspace --all-targets -- -D warnings` bersih,
  suite M1 tetap hijau (m1_execute 5, m1_pool 2).
