# PHPX supported PHP version matrix (M3 developer preview)

`phpx doctor` reports the actual `php --version` and marks `8.3 / 8.4 / 8.5` as target.
`phpx runtime list|inspect` shows `php_target` (from config) vs `php_actual` (from PATH or `PHPX_PHP_BIN`).

## M3 test matrix

| PHP | Status (M3) | Notes |
|-----|-------------|-------|
| 8.3 | Supported, CI-tested where available | `phpx doctor` -> yes. Vanilla fixture passes. |
| 8.4 | Supported, default `php_version = "8.4"` | Docker runtime base `php:8.4-cli-bookworm`. Vanilla fixture passes. |
| 8.5 | Best-effort | `doctor` marks yes if `8.5` string found; full CI matrix is M4 hardening. |
| < 8.3 (8.1, 8.2, 7.x) | Unverified | `doctor` prints `unverified`. May work for vanilla fixture but not claimed. File an issue with `phpx doctor --json` output. |
| PHP forks / custom builds | Unverified | Must provide `php` CLI in PATH or set `PHPX_PHP_BIN`. NTS/ZTS both tolerated in M3 (one-shot CLI); embed SAPI audit is M4/M6. |

## Extensions (M3)

`phpx doctor` checks (warning, not fatal unless `php` itself missing):

- `json`, `mbstring`, `curl`, `openssl`, `pdo` -> `found` or `missing (warning)`.

Vanilla fixture needs only core. Framework fixtures (Composer/Laravel/WordPress) are M4 compatibility work and will extend this table with required extensions per app.

## How to select PHP

```bash
php --version
PHPX_PHP_BIN=/usr/bin/php8.4 phpx doctor
PHPX_PHP_BIN=/usr/bin/php8.4 phpx serve --config phpx.toml
```

In Docker, the image pins PHP via `php:8.4-cli-bookworm`. Check with:

```bash
docker run --rm phpx:0.1.0-dev php --version
```

## M4 outlook (not M3)

- Publish per-version CI results (8.3/8.4/8.5 x extensions).
- Document SAPI/extension gaps for Laravel/WordPress.
- No performance ranking by PHP version until reproducible baseline benchmarks exist (see scripts/bench.sh).
