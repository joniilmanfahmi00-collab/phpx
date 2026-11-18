# PHPX compatibility matrix (M4)

Developer preview v0.1. Scope: Process Runtime (one-shot `php <entry>` per request).
`Failed` = terminal for that request attempt with diagnostic reason + restart-budget
accounting; `Stopped` = drained pool, no new work (SPEC 5.1 + ADR-0014 alignment).
Crash recovery = next request gets a fresh process; no cross-request state by construction.

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
| `m4_compat` (vanilla/composer/laravel/wp/woo + /metrics) | PASS (process green; embedded deferred) |
| `m3_cli` (`doctor/metrics/queue/scheduler/cache/storage`) | PASS (6/6) |
