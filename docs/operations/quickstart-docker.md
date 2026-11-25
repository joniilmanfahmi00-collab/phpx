# Quickstart Docker PHPX (pratinjau developer M3)

Image berjalan sebagai non-root (`phpx`, uid 10001). Status: pratinjau developer v0.1, bukan SLA produksi. Lihat limitations.md.

## 1. Build

```bash
docker build -t phpx:0.1.0-dev .
docker images | grep phpx
```

Stage builder adalah `rust:1.99.1-bookworm`; stage runtime adalah `php:8.4-cli-bookworm` + binary `phpx` + fixture vanilla.

## 2. Jalankan

```bash
docker run --rm -p 8080:8080 phpx:0.1.0-dev
# binary menjalankan: phpx serve --config /app/phpx.toml (host 0.0.0.0:8080, fixture vanilla)
```

Di terminal lain:

```bash
curl -i http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/
curl -i http://127.0.0.1:8080/style.css
```

Harapkan `200` + header `x-request-id`. `HEALTHCHECK` mengenai `/healthz` tiap 15s.

## 3. Cek non-root

```bash
docker run --rm phpx:0.1.0-dev whoami
# -> phpx
docker run --rm phpx:0.1.0-dev id
# -> uid=10001(phpx) gid=10001(phpx)
```

## 4. App kustom (bind mount)

Mount app Anda di `/srv/app` dengan `public/index.php`:

```bash
docker run --rm -p 8080:8080 \
  -v /path/to/myapp:/srv/app:ro \
  -e PHPP_BIN=php \
  phpx:0.1.0-dev \
  phpx serve --config /app/phpx.toml
```

Untuk docroot kustom, buat `phpx.toml` sendiri (lihat configuration.md) dan mount:

```bash
docker run --rm -p 8080:8080 \
  -v /path/to/myapp:/srv/app:ro \
  -v /path/to/phpx.toml:/app/phpx.toml:ro \
  phpx:0.1.0-dev
```

Contoh `phpx.toml` host:

```toml
schema_version = 1
[server]
host = "0.0.0.0"
port = 8080
tls = false
request_body_limit = "16MB"
[application]
root = "/srv/app"
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "process"
php_version = "8.4"
[runtime.process]
workers = 4
request_timeout_seconds = 30
startup_timeout_seconds = 10
memory_limit = "256M"
[runtime.embedded]
enabled = false
```

## 5. Smoke

```bash
bash scripts/smoke-docker.sh
# membangun image bila hilang, berjalan detached, mengecek healthz/readyz, /, statis, traversal 400, lalu berhenti.
```

## Catatan

- Terminasi TLS di M3 bersifat in-app via Rustls (`tls=true` + PEM) atau di reverse proxy Anda. HTTP polos di dalam jaringan Docker wajar untuk quickstart.
- Tanpa volume yang wajib untuk fixture vanilla; `/app/data` writable untuk DB queue (M5, sqlite/redis).
- Endpoint Prometheus `/metrics` tersedia sejak M4; `docker logs` tetap berguna untuk access log terstruktur.
