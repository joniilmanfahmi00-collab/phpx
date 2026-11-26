# Matriks kompatibilitas PHPX (M4 + P0-D + P2)

Developer preview v0.1. Lingkup: Process Runtime one-shot (`php <entry>` per
request) + persistent supervised + embedded single-flight (opt-in) + smoke
app asli P0-D (Laravel 12, WordPress 7.1 + WooCommerce 11.3, gateway process).
`Failed` = terminal untuk percobaan request itu dengan alasan diagnostik +
akuntansi restart-budget; `Stopped` = pool terdrain, tanpa kerja baru
(penyelarasan SPEC 5.1 + ADR-0014).
Crash recovery = request berikut mendapat proses fresh; tanpa state
lintas-request secara konstruksi (one-shot) atau via resync SAPI (persistent).
Server yang diuji selalu PHPX sendiri (`phpx serve` / `spawn_test_server`).
Apache/nginx/php-fpm/Laragon/WP Studio tidak pernah menjadi server yang diuji.

## Aplikasi asli (P0-D, dilayani via `phpx serve` — bukan Apache)

| App (upstream, sumber tak diubah) | Provisioning | Bukti | Lingkup + batas |
|---|---|---|---|
| Laravel 12 (`laravel/laravel:^12.0`, sqlite, migrated) | `scripts/setup-laravel-smoke.sh` / `.ps1`, atau otomatis oleh test (butuh `composer`); override dir dengan `PHPX_LARAVEL_DIR` | `p0d_laravel` (5/5): welcome + tanpa penanda gagal; body 404; JSON 405 (`Accept: application/json`); session persist sisi-server (baris sqlite +1/request); multipart POST diparsing shim SAPI (rute tetap 405 sesuai desain, tanpa crash) | Smoke + integrasi (routing, error JSON, session, parse multipart). Status set-PHP TIDAK dipropagasi (404/405 tiba sebagai 200). Set-Cookie ditelan CLI SAPI (id session tak teramati di wire) |
| WordPress 7.1 (`wordpress.org/latest.zip`, install wp-cli, MySQL 8.4) + WooCommerce 11.3 (plugin upstream asli; produk skrip `Smoke Pixel` 25) | `scripts/mysql-smoke.sh start` + `scripts/setup-wordpress-smoke.sh` (`.ps1` di Windows; CI: service mariadb + wp-cli); override dir dengan `PHPX_WORDPRESS_DIR`; URL install harus `http://127.0.0.1:18312` (port gateway test tetap, `PHPX_WP_URL`); app password di `.phpx-app-password` (artefak deployment, di-gitignore) | `p0d_wordpress` (13/13: front/search/pretty/login/REST/comments/media WP di atas, plus loop toko + URL pretty produk + dok redirect-swallow add-to-cart/cart/checkout + katalog JSON `wc/v3/products` dengan auth app-password, `woocommerce_rest_cannot_view` tanpa auth) | Smoke + integrasi (routing, query, JSON, form POST, cookies-masuk, auth, tulis DB). Status/Location set-PHP TIDAK dipropagasi; hit `?p=N`, `?add-to-cart`, checkout-kosong kena redirect-swallow (200 depan/kosong). Byte mentah upload diblokir: `php://input` kosong di CLI SAPI (dipin sebagai kode error JSON); multipart/form-data mengisi `$_POST`/`$_FILES` (file tmp, `move_uploaded_file()` tetap false — baca `tmp_name` atau `rename()`). `$_POST` urlencoded + multipart + `PHP_AUTH_*`/`HTTP_AUTHORIZATION` + `REMOTE_ADDR`/`SERVER_NAME`/`SERVER_PORT` kini diemulasi (lihat sapi-audit.md) |

Terverifikasi 2026-11-22: Laravel Framework 12.73.0 (PHP 8.3.30, dev Windows) +
WordPress 7.1.5 (PHP 8.3.30 + MySQL 8.4.3, dev Windows).
Catat versi lokal setiap run; test menerima 12.x / 7.x apa pun.

## Fixture (semua dilayani via `phpx serve`, tanpa ubah sumber app)

| Fixture | Dir | GET / | Query/header/cookie/body | Statik | Catatan |
|---------|-----|-------|----------------------------|--------|-------|
| vanilla-php | `examples/vanilla-php` | 200 `hello from phpx vanilla` | `rid` beda per request; `isolate.php` x2 = `clean` keduanya | `style.css` 200 | Referensi isolasi + crash-recovery |
| composer-autoload | `examples/composer-autoload` | 200 `composer-autoload ok` | `?name=ara` -> `greeter=hello ara` (`$_GET` via bootstrap); round-trip cookie/header/body | `style.css` 200 | Stub `vendor/autoload.php` (tanpa network); PSR-4 `App\\` |
| laravel-minimal (sim) | `examples/laravel-minimal` | 200 `laravel-minimal ok` | JSON `/api/hello`; header middleware; echo cookie/body | `style.css` 200 | Tanpa unduh framework; simulasi routing/env/middleware |
| wordpress-minimal (sim) | `examples/wordpress-minimal` | 200 `wordpress-minimal ok` + loop | `?p=2`, `?s=hello` | `style.css` 200 | Konstanta `WP_SIM`, shortcode `[phpx]` -> `PHPX` |
| woocommerce-minimal (sim, setelah baseline WP) | `examples/woocommerce-minimal` | 200 `woocommerce-minimal ok` | `?add_to_cart=101` -> cart/total; `?checkout=1` JSON | `style.css` 200 | Cart via query, total dalam sen |

Bukti: `cargo test -p phpx-server --test m4_compat` (isolasi/crash,
composer+SAPI, laravel, wp+woo, `/metrics`).

## Versi PHP (teruji M4)

| PHP | SAPI yang diuji | Hasil |
|-----|-----------------|--------|
| 8.3.30 (cli, dev Windows) | `cli` one-shot | PASS (lokal setara CI: suite M4 hijau) |
| 8.4 / 8.5 | `cli` one-shot | Target per `php_version`; verifikasi dengan `phpx doctor` (warning belum terverifikasi bila mismatch) |
| < 8.3 | apa pun | Tidak didukung (doctor warning; target `php_version` 8.3+) |

## Kombinasi ekstensi (M4)

`phpx doctor` mengaudit: `json`, `mbstring`, `curl`, `openssl`, `pdo`,
`pdo_sqlite`, `fileinfo`, `filter`, `dom`, `gd`, `intl`. Hilang = warning
(sebagian app butuh), bukan hard error kecuali `php` sendiri hilang.
Runbook matriks penuh: `php -m` di host target vs kebutuhan app; catat gap di sini.

## Tidak didukung / batas eksplisit

- SAPI: hanya worker `cli` yang didukung (one-shot fresh per request atau
  persistent supervised). `fpm`/`cgi`/handler Apache TIDAK dipakai; gateway
  men-spawn `php <entry>` langsung (lihat `sapi-audit.md`).
- `php://input` kosong di CLI; pakai `phpx_input()` (bootstrap) atau
  `file_get_contents("php://input") ?: file_get_contents("php://stdin")`.
- Worker persistent tersedia sejak P0-B (supervised, handshake, recycle
  graceful); cache lintas-request OPcache bukan klaim.
- Backend queue/scheduler/cache/storage tersedia sejak M5 (bukan stub).
- Runtime embedded: experimental opt-in sejak P2 (9/9 gerbang linked,
  `runtime.mode=embedded`); default tetap process.
- HTTP/2: ditunda, tidak diklaim.

## Verifikasi M7 (RC)

| Suite | Hasil |
|-------|--------|
| `phpx-server --lib` (redaksi + matriks traversal + traceparent) | PASS (5/5) |
| `m7_security` (raw-socket 400, secret soak, symlink 403/404, Dockerfile non-root) | PASS (4/4) |
| `m7_resilience` (saturasi terbatas + metrics teramati + recovery, gerbang config) | PASS (2/2) |
| `m4_compat` (vanilla/composer/laravel/wp/woo + /metrics) | PASS (process hijau; paritas + gateway embedded via `p2_embedded_gateway`, vanilla live di linked) |
| `m3_cli` (`doctor/metrics/queue/scheduler/cache/storage`) | PASS (6/6) |
| `p2_embedded_gateway` (pool embedded + routing + timeout eksplisit; stub fail-closed, linked live) | PASS (5/5) |
| `p0d_laravel` (Laravel 12 asli: welcome, 404, JSON 405, session, multipart) | PASS (5/5) di Windows nativ dan CI Linux. Catatan WSL: mount `/mnt/d` (9p) yang terdegradasi membuat test ini timeout (`p9_client_rpc`, byte identik lolos 0,7 dtk di ext4 dan 5/5 di Windows) — artefak filesystem, bukan regresi; ulangi di mount sehat atau andalkan CI |
| `p0d_wordpress` (WordPress 7.1 asli + WooCommerce 11.3: front, search, pretty, login, REST, comments, media auth, shop, product, dok cart/checkout, wc/v3; butuh install terprovisi + MySQL) | PASS (13/13), SKIP lantang bila tak terprovisi |
| `p1a_resilience` (idle crash, sirkuit, drain in-flight x2, app-exit resync, reload, dok leak, timeline recovery, short soak) | PASS (9/9) |
| `p0b_persistent` + `persistent` (pool terawasi, state machine, guardian) | PASS (3+14) |

Lihat `docs/operations/resilience.md` untuk matriks failure-injection dan semantik timeline recovery.
