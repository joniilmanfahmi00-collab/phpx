# PHPX Docker quickstart (M3 developer preview)

Image runs as non-root (`phpx`, uid 10001). Status: v0.1 developer preview, not a production SLA. See limitations.md.

## 1. Build

```bash
docker build -t phpx:0.1.0-dev .
docker images | grep phpx
```

Builder stage is `rust:1.99.1-bookworm`; runtime stage is `php:8.4-cli-bookworm` + `phpx` binary + vanilla fixture.

## 2. Run

```bash
docker run --rm -p 8080:8080 phpx:0.1.0-dev
# binary runs: phpx serve --config /app/phpx.toml (host 0.0.0.0:8080, vanilla fixture)
```

In another terminal:

```bash
curl -i http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/
curl -i http://127.0.0.1:8080/style.css
```

Expect `200` + `x-request-id` header. `HEALTHCHECK` hits `/healthz` every 15s.

## 3. Non-root check

```bash
docker run --rm phpx:0.1.0-dev whoami
# -> phpx
docker run --rm phpx:0.1.0-dev id
# -> uid=10001(phpx) gid=10001(phpx)
```

## 4. Custom app (bind mount)

Mount your app at `/srv/app` with `public/index.php`:

```bash
docker run --rm -p 8080:8080 \
  -v /path/to/myapp:/srv/app:ro \
  -e PHPP_BIN=php \
  phpx:0.1.0-dev \
  phpx serve --config /app/phpx.toml
```

For a custom docroot, create your own `phpx.toml` (see configuration.md) and mount it:

```bash
docker run --rm -p 8080:8080 \
  -v /path/to/myapp:/srv/app:ro \
  -v /path/to/phpx.toml:/app/phpx.toml:ro \
  phpx:0.1.0-dev
```

Example host `phpx.toml`:

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
# builds image if missing, runs detached, checks healthz/readyz, /, static, traversal 400, then stops.
```

## Notes

- TLS termination in M3 is in-app via Rustls (`tls=true` + PEM) or at your reverse proxy. Plain HTTP inside Docker network is normal for quickstart.
- No volumes are required for the vanilla fixture; `/app/data` is writable for future queue DB (M5).
- Prometheus endpoint deferred to M4; use `docker logs` for structured access logs.
