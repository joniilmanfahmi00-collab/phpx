# PHPX compatibility matrix (M4 + P0-D)

Developer preview v0.1. Scope: Process Runtime one-shot `php <entry>` per request
+ P0-D real-application smoke (Laravel 12, gateway one-shot).
`Failed` = terminal for that request attempt with diagnostic reason + restart-budget
accounting; `Stopped` = drained pool, no new work (SPEC 5.1 + ADR-0014 alignment).
Crash recovery = next request gets a fresh process; no cross-request state by construction.
The server under test is always PHPX itself (`phpx serve` / `spawn_test_server`).
Apache/nginx/php-fpm/Laragon/WP Studio are never the server under test.

## Real applications (P0-D, served via `phpx serve` — never Apache)

| App (upstream, unmodified source) | Provisioning | Evidence | Scope + limits |
|---|---|---|---|
| Laravel 12 (`laravel/laravel:^12.0`, sqlite, migrated) | `scripts/setup-laravel-smoke.sh` / `.ps1`, or auto by test (needs `composer`); override dir with `PHPX_LARAVEL_DIR` | `p0d_laravel` (5/5): welcome + no-failure-markers; 404 body; JSON 405 (`Accept: application/json`); sessions persist server-side (sqlite rows +1/request); multipart POST parsed by the SAPI shim (route still 405 as designed, no crash, 837KB debug page streams fine) | Smoke + integration (routing, error JSON, sessions, multipart parse). PHP-set status NOT propagated (404/405 arrive as 200). Set-Cookie swallowed by CLI SAPI (session id not observable on wire) |
| WordPress 7.1 (`wordpress.org/latest.zip`, wp-cli install, MySQL 8.4) + WooCommerce 11.3 (real upstream plugin; scripted product `Smoke Pixel` 25) | `scripts/mysql-smoke.sh start` + `scripts/setup-wordpress-smoke.sh` (`.ps1` on Windows; CI: mariadb service + wp-cli); override dir with `PHPX_WORDPRESS_DIR`; install URL must be `http://127.0.0.1:18312` (fixed test gateway port, `PHPX_WP_URL`); app password in `.phpx-app-password` (deployment artifact, gitignored) | `p0d_wordpress` (13/13: WP front/search/pretty/login/REST/comments/media above, plus shop loop + product pretty URL + add-to-cart/cart/checkout redirect-swallow docs + `wc/v3/products` catalog JSON with app-password auth, unauth `woocommerce_rest_cannot_view`) | Smoke + integration (routing, query, JSON, form POST, cookies-in, auth, DB writes). PHP-set status/Location NOT propagated; `?p=N`, `?add-to-cart`, empty-checkout hits redirect-swallow (empty/front 200s). Upload raw bytes blocked: `php://input` empty under CLI SAPI (pinned as JSON error codes); multipart/form-data populates `$_POST`/`$_FILES` (tmp files, `move_uploaded_file()` stays false — read `tmp_name` or `rename()`). `$_POST` urlencoded + multipart + `PHP_AUTH_*`/`HTTP_AUTHORIZATION` + `REMOTE_ADDR`/`SERVER_NAME`/`SERVER_PORT` now emulated (see sapi-audit.md) |

Verified 2026-11-22: Laravel Framework 12.73.0 (PHP 8.3.30, Windows dev) +
WordPress 7.1.5 (PHP 8.3.30 + MySQL 8.4.3, Windows dev).
Record the local versions after each run; the tests accept any 12.x / 7.x.

## Fixtures (all served via `phpx serve`, no app source changes)

| Fixture | Dir | GET / | Query/headers/cookies/body | Static | Notes |
|---------|-----|-------|----------------------------|--------|-------|
| vanilla-php | `examples/vanilla-php` | 200 `hello from phpx vanilla` | `rid` differs per request; `isolate.php` x2 = `clean` both | `style.css` 200 | Isolation + crash-recovery reference |
| composer-autoload | `examples/composer-autoload` | 200 `composer-autoload ok` | `?name=ara` -> `greeter=hello ara` (`$_GET` via bootstrap); cookie/header/body round-trip | `style.css` 200 | Stub `vendor/autoload.php` (no network); PSR-4 `App\\` |
| laravel-minimal (sim) | `examples/laravel-minimal` | 200 `laravel-minimal ok` | `/api/hello` JSON; middleware header; cookie/body echo | `style.css` 200 | No framework download; routing/env/middleware simulation |
| wordpress-minimal (sim) | `examples/wordpress-minimal` | 200 `wordpress-minimal ok` + loop | `?p=2`, `?s=hello` | `style.css` 200 | `WP_SIM` constants, shortcode `[phpx]` -> `PHPX` |
| woocommerce-minimal (sim, after WP baseline) | `examples/woocommerce-minimal` | 200 `woocommerce-minimal ok` | `?add_to_cart=101` -> cart/total; `?checkout=1` JSON | `style.css` 200 | Cart via query, totals in cents |

Evidence: `cargo test -p phpx-server --test m4_compat` (5 tests: isolation/crash,
composer+SAPI, laravel, wp+woo, `/metrics`).

## PHP versions (M4 tested)

| PHP | SAPI under test | Result |
|-----|-----------------|--------|
| 8.3.30 (cli, Windows dev) | `cli` one-shot | PASS (CI-equivalent local: M4 suite green) |
| 8.4 / 8.5 | `cli` one-shot | Target per `php_version`; verify with `phpx doctor` (unverified warning if mismatch) |
| < 8.3 | any | Unsupported (doctor warns; `php_version` target is 8.3+) |

## Extension combos (M4)

`phpx doctor` audits: `json`, `mbstring`, `curl`, `openssl`, `pdo`,
`pdo_sqlite`, `fileinfo`, `filter`, `dom`, `gd`, `intl`. Missing = warning
(some apps need it), never a hard error unless `php` itself is missing.
Full matrix runbook: `php -m` on target host vs app requirements; record gaps here.

## Unsupported / explicit limits

- SAPI: only `cli` one-shot is supported. `fpm`/`cgi`/Apache handler are NOT used;
  the gateway spawns `php <entry>` directly (see `sapi-audit.md`).
- `php://input` is empty under CLI; use `phpx_input()` (bootstrap) or
  `file_get_contents("php://input") ?: file_get_contents("php://stdin")`.
- Persistent workers / OPcache cross-request caching: not provided in M4
  (one-shot = fresh interpreter per request; slower bootstrap, stronger isolation).
- Queue/scheduler/cache/storage backends: deferred to M5 (CLI stubs exit 2).
- Embedded runtime: experimental-disabled until M6.
- HTTP/2: deferred, not claimed.

## M7 verification (RC)

| Suite | Result |
|-------|--------|
| `phpx-server --lib` (redaction + traversal matrix + traceparent) | PASS (5/5) |
| `m7_security` (raw-socket 400, secret soak, symlink 403/404, Dockerfile non-root) | PASS (4/4) |
| `m7_resilience` (saturation bounded + metrics observable + recovery, config gates) | PASS (2/2) |
| `m4_compat` (vanilla/composer/laravel/wp/woo + /metrics) | PASS (process green; embedded parity + gateway via `p2_embedded_gateway`, vanilla live on linked) |
| `m3_cli` (`doctor/metrics/queue/scheduler/cache/storage`) | PASS (6/6) |
| `p0d_laravel` (real Laravel 12: welcome, 404, JSON 405, sessions, multipart) | PASS (5/5) |
| `p0d_wordpress` (real WordPress 7.1 + WooCommerce 11.3: front, search, pretty, login, REST, comments, media auth, shop, product, cart/checkout docs, wc/v3; needs provisioned install + MySQL) | PASS (13/13), SKIP loudly when unprovisioned |
| `p1a_resilience` (idle crash, circuit, in-flight drain x2, app-exit resync, reload, leak doc, recovery timeline, short soak) | PASS (9/9) |
| `p0b_persistent` + `persistent` (supervised pool, state machine, guardian) | PASS (3+14) |

See `docs/operations/resilience.md` for the failure-injection matrix and recovery-timeline semantics.
