# PHP SAPI compatibility audit (M4 + P0-D)

## What the gateway actually does

- Spawns `php -d auto_prepend_file=<phpx-bootstrap> <entrypoint>` per request
  (one-shot, cwd = app root). No FPM, no CGI, no Apache module.
- Bridges via environment: `PHPX_REQUEST_ID`, `PHPX_METHOD`, `PHPX_PATH`,
  `PHPX_QUERY`, `PHPX_HEADERS_JSON`, `PHPX_PROTOCOL`. Body is piped on stdin.
- `src/bootstrap.php` (embedded, materialized to temp) emulates CGI-like state:
  `$_GET` (from `PHPX_QUERY`), `$_COOKIE` (from `Cookie` header),
  `$_POST` for `application/x-www-form-urlencoded` bodies (parsed from stdin),
  `$_SERVER["REQUEST_METHOD"/"REQUEST_URI"/"QUERY_STRING"/"HTTP_*"/"CONTENT_*"/"PHP_AUTH_*"/"REMOTE_ADDR"]`,
  `HTTP_AUTHORIZATION` passthrough, `getallheaders()` polyfill, and
  `phpx_input()` body helper.
- PHP-level heap limit: `-d memory_limit=<limit>` from `runtime.process.memory_limit`
  (P0-C, real enforcement; heap only, not OS RSS).
- Stdout/stderr are drained concurrently (P0-D fix): responses larger than the
  OS pipe buffer (~64KB, e.g. Laravel welcome ~80KB) no longer deadlock.

## Supported

- SAPI `cli` only. Verified: `php -r "echo php_sapi_name();"` -> `cli`.
  `phpx doctor` reports `php-sapi` OK only for `cli`.
- Superglobals above + `$_REQUEST` merge + `php://stdin` body.
  Proven by `m4_compat`: `?name=ara`, cookies, `User-Agent`, POST bodies.

## Unsupported (explicit, not bugs)

- `php-fpm` / `php-cgi` / `mod_php`: never spawned; apps must not assume FPM
  status pages, `fastcgi_*` functions, or `.user.ini` FPM semantics.
- `php://input`: empty under CLI. Workaround (documented, used by fixtures):
  `function_exists("phpx_input") ? phpx_input() : file_get_contents("php://input")`.
- Multipart/form-data is parsed in PHP (RC track): `$_POST` (incl. `a[]`,
  `a[b]` shapes) and `$_FILES` (plain, `f[]`, `a[b]` shapes) are populated
  from the raw body in both one-shot (`bootstrap.php`) and persistent
  (`worker-persistent.php`) workers; temp files via `tempnam()` tracked in
  `$GLOBALS["PHPX_TMP_FILES"]` and unlinked after the request (shutdown
  function one-shot, next-request cleanup persistent). Caps: boundary <= 200
  chars, <= 200 parts, boundary match requires a valid `--`/`CRLF` trailer so
  lookalike bytes inside file content do not split. Raw bytes stay available
  via `phpx_input()`.
- `move_uploaded_file()` / `is_uploaded_file()`: always false under CLI SAPI
  (PHP only trusts real SAPI uploads). Read bytes via
  `file_get_contents($_FILES[...]["tmp_name"])` or `rename()` the temp file.
  Real apps that call `move_uploaded_file()` (e.g. WP `wp_handle_upload`)
  need an app-side bypass; fixture `examples/vanilla-php/public/upload.php`
  pins the supported contract.
- Apps that terminate the process (`exit`/`die`, e.g. WordPress `wp_redirect`
  and REST serving paths) kill a persistent worker: the supervisor maps it to
  500 + restart with a new incarnation and resyncs the stream
  (pinned by `p1a: app_exit_kills_worker_and_pool_resyncs`). One-shot captures
  the flushed output as the response body instead. Prefer one-shot for apps
  that exit per request.
  This blocks REST/file-upload request *bodies* sent as raw octets
  (e.g. WP media `rest_upload_no_data` for `Content-Type: image/png`).
  Multipart uploads populate `$_FILES` (see above); only the raw
  `php://input` path stays empty.
- `$_FILES` under CLI SAPI: populated by the PHPX parser (see above), not by
  PHP itself. Routing still works as before (Laravel multipart POST ->
  graceful 405); `move_uploaded_file()` stays false — read `tmp_name` or
  `rename()` it.
- `$_SESSION` persistence: not emulated (no session file/cookie round-trip
  through the gateway). Apps that store sessions server-side (Laravel database
  driver) still persist rows and can be asserted DB-side; the session *cookie*
  itself is swallowed with all other `Set-Cookie` output.
- `$_SERVER["REMOTE_ADDR"]` defaults to `127.0.0.1`; `["SERVER_NAME"]`/`["SERVER_PORT"]`
  derive from the `Host` header (IPv6-aware); `HTTP_AUTHORIZATION` is passed
  through and `Authorization: Basic` maps to `PHP_AUTH_USER`/`PHP_AUTH_PW`
  (CGI parity; Bearer/other schemes untouched). The true client IP is not
  forwarded — document before trusting any of these for auth/rate-limit decisions.
- Long-lived resources (DB pools, OPcache) do not persist across requests
  (one-shot). This is the isolation tradeoff: safe by default, bootstrap cost
  per request.
- PHP-set response status/headers are NOT propagated under CLI (P0-D finding):
  `header()` and `http_response_code()` are no-ops in `cli` SAPI, so the
  gateway returns 200 with the framework's rendered body. Examples: Laravel's
  404 page arrives as HTTP 200 containing "Not Found"
  (pinned by `p0d_laravel::laravel_unknown_route_reports_not_found_body`);
  with WP pretty permalinks, `?p=N` shortlink answers become empty-200 bodies
  because `redirect_canonical()` emits `Location` + `exit` and the header is
  dropped. Crash (non-zero exit) -> 500, timeout -> 504 are gateway-mapped
  and unaffected.

## How to audit a new host

1. `phpx doctor` (checks `php`, `php-sapi`, 11 extensions, config, OS).
2. `php -m` vs app requirement list; record gaps in `compatibility-matrix.md`.
3. `cargo test -p phpx-server --test m4_compat` against the fixture set.
