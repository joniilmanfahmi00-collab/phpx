# PHP SAPI compatibility audit (M4)

## What the gateway actually does

- Spawns `php -d auto_prepend_file=<phpx-bootstrap> <entrypoint>` per request
  (one-shot, cwd = app root). No FPM, no CGI, no Apache module.
- Bridges via environment: `PHPX_REQUEST_ID`, `PHPX_METHOD`, `PHPX_PATH`,
  `PHPX_QUERY`, `PHPX_HEADERS_JSON`, `PHPX_PROTOCOL`. Body is piped on stdin.
- `src/bootstrap.php` (embedded, materialized to temp) emulates CGI-like state:
  `$_GET` (from `PHPX_QUERY`), `$_COOKIE` (from `Cookie` header),
  `$_SERVER["REQUEST_METHOD"/"REQUEST_URI"/"QUERY_STRING"/"HTTP_*"/"CONTENT_*"]`,
  `getallheaders()` polyfill, and `phpx_input()` body helper.

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
- `$_FILES` / multipart parsing, `$_SESSION` persistence, `getenv("FCGI_*")`:
  not emulated in M4. Multipart bodies arrive raw on stdin; frameworks that
  need `$_FILES` require M5+ SAPI work.
- Long-lived resources (DB pools, OPcache) do not persist across requests
  (one-shot). This is the isolation tradeoff: safe by default, bootstrap cost
  per request.

## How to audit a new host

1. `phpx doctor` (checks `php`, `php-sapi`, 11 extensions, config, OS).
2. `php -m` vs app requirement list; record gaps in `compatibility-matrix.md`.
3. `cargo test -p phpx-server --test m4_compat` against the fixture set.
