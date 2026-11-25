# Audit kompatibilitas SAPI PHP (M4 + P0-D)

## Yang benar-benar dilakukan gateway

- Men-spawn `php -d auto_prepend_file=<phpx-bootstrap> <entrypoint>` per request
  (one-shot, cwd = app root). Tanpa FPM, tanpa CGI, tanpa modul Apache.
- Menjembatani via environment: `PHPX_REQUEST_ID`, `PHPX_METHOD`, `PHPX_PATH`,
  `PHPX_QUERY`, `PHPX_HEADERS_JSON`, `PHPX_PROTOCOL`. Body dipipa lewat stdin.
- `src/bootstrap.php` (embedded, dimaterialisasi ke temp) mengemulasi state
  ala-CGI: `$_GET` (dari `PHPX_QUERY`), `$_COOKIE` (dari header `Cookie`),
  `$_POST` untuk body `application/x-www-form-urlencoded` (diparsing dari stdin),
  `$_SERVER["REQUEST_METHOD"/"REQUEST_URI"/"QUERY_STRING"/"HTTP_*"/"CONTENT_*"/"PHP_AUTH_*"/"REMOTE_ADDR"]`,
  passthrough `HTTP_AUTHORIZATION`, polyfill `getallheaders()`, dan helper
  body `phpx_input()`.
- Batas heap level-PHP: `-d memory_limit=<limit>` dari `runtime.process.memory_limit`
  (P0-C, enforcement nyata; hanya heap, bukan OS RSS).
- Stdout/stderr dikuras konkuren (fix P0-D): respons lebih besar dari buffer
  pipe OS (~64KB, mis. welcome Laravel ~80KB) tidak lagi deadlock.

## Didukung

- Hanya SAPI `cli`. Terverifikasi: `php -r "echo php_sapi_name();"` -> `cli`.
  `phpx doctor` melaporkan `php-sapi` OK hanya untuk `cli`.
- Superglobal di atas + merge `$_REQUEST` + body `php://stdin`.
  Terbukti oleh `m4_compat`: `?name=ara`, cookie, `User-Agent`, body POST.

## Tidak didukung (eksplisit, bukan bug)

- `php-fpm` / `php-cgi` / `mod_php`: tidak pernah di-spawn; app tidak boleh
  mengasumsikan halaman status FPM, fungsi `fastcgi_*`, atau semantik `.user.ini` FPM.
- `php://input`: kosong di CLI. Workaround (terdokumentasi, dipakai fixture):
  `function_exists("phpx_input") ? phpx_input() : file_get_contents("php://input")`.
- Multipart/form-data diparsing di PHP (jalur RC): `$_POST` (termasuk bentuk `a[]`,
  `a[b]`) dan `$_FILES` (polos, `f[]`, bentuk `a[b]`) diisi dari body mentah
  baik di worker one-shot (`bootstrap.php`) maupun persistent
  (`worker-persistent.php`); file tmp via `tempnam()` dilacak di
  `$GLOBALS["PHPX_TMP_FILES"]` dan di-unlink setelah request (fungsi shutdown
  untuk one-shot, cleanup next-request untuk persistent). Cap: boundary <= 200
  karakter, <= 200 part, kecocokan boundary mensyaratkan trailer `--`/`CRLF`
  valid agar byte mirip di dalam konten file tidak membelah. Byte mentah tetap
  tersedia via `phpx_input()`.
- `move_uploaded_file()` / `is_uploaded_file()`: selalu false di CLI SAPI
  (PHP hanya memercayai upload SAPI asli). Baca byte via
  `file_get_contents($_FILES[...]["tmp_name"])` atau `rename()` file tmp.
  App asli yang memanggil `move_uploaded_file()` (mis. `wp_handle_upload` WP)
  butuh bypass sisi-app; fixture `examples/vanilla-php/public/upload.php`
  mem-pin kontrak yang didukung.
- App yang menghentikan proses (`exit`/`die`, mis. `wp_redirect` WordPress dan
  jalur serving REST) membunuh worker persistent: supervisor memetakannya ke
  500 + restart dengan inkarnasi baru dan me-resync stream
  (dipin oleh `p1a: app_exit_kills_worker_and_pool_resyncs`). One-shot menangkap
  output yang ter-flush sebagai body respons. Pilih one-shot untuk app yang
  exit per request.
  Ini memblokir body request REST/file-upload yang dikirim sebagai oktet mentah
  (mis. `rest_upload_no_data` media WP untuk `Content-Type: image/png`).
  Upload multipart mengisi `$_FILES` (lihat atas); hanya jalur `php://input`
  mentah yang tetap kosong.
- `$_FILES` di CLI SAPI: diisi oleh parser PHPX (lihat atas), bukan oleh PHP
  sendiri. Routing tetap bekerja seperti sebelumnya (multipart POST Laravel ->
  405 graceful); `move_uploaded_file()` tetap false — baca `tmp_name` atau
  `rename()`.
- Persistensi `$_SESSION`: tidak diemulasi (tanpa round-trip file session/cookie
  lewat gateway). App yang menyimpan session sisi-server (driver database
  Laravel) tetap menulis baris dan dapat diasert sisi-DB; *cookie* session
  sendiri ditelan bersama semua output `Set-Cookie` lain.
- `$_SERVER["REMOTE_ADDR"]` default `127.0.0.1`; `["SERVER_NAME"]`/`["SERVER_PORT"]`
  diturunkan dari header `Host` (sadar IPv6); `HTTP_AUTHORIZATION` diteruskan
  dan `Authorization: Basic` dipetakan ke `PHP_AUTH_USER`/`PHP_AUTH_PW`
  (paritas CGI; skema Bearer/lain tak tersentuh). IP klien sejati tidak
  diteruskan — dokumentasikan sebelum memercayai salah satunya untuk keputusan auth/rate-limit.
- Resource berumur-panjang (pool DB, OPcache) tidak bertahan lintas request
  (one-shot). Ini tradeoff isolasi: aman default, ongkos bootstrap per request.
- Status/header respons set-PHP TIDAK dipropagasi di CLI (temuan P0-D):
  `header()` dan `http_response_code()` adalah no-op di SAPI `cli`, jadi
  gateway mengembalikan 200 dengan body renderan framework. Contoh: halaman
  404 Laravel tiba sebagai HTTP 200 berisi "Not Found"
  (dipin oleh `p0d_laravel::laravel_unknown_route_reports_not_found_body`);
  dengan pretty permalink WP, jawaban shortlink `?p=N` menjadi body 200 kosong
  karena `redirect_canonical()` mengemis `Location` + `exit` dan header-nya
  dibuang. Crash (exit non-nol) -> 500, timeout -> 504 dipetakan gateway dan
  tidak terpengaruh.

## Cara mengaudit host baru

1. `phpx doctor` (cek `php`, `php-sapi`, 11 ekstensi, config, OS).
2. `php -m` vs daftar kebutuhan app; catat gap di `compatibility-matrix.md`.
3. `cargo test -p phpx-server --test m4_compat` terhadap set fixture.
