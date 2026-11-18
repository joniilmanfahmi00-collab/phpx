# PHPX v0.1 developer preview - Docker image (non-root, M3).
# Build:  docker build -t phpx:0.1.0-dev .
# Run:    docker run --rm -p 8080:8080 phpx:0.1.0-dev
# Quickstart: see docs/operations/quickstart-docker.md

# ---- builder ----
FROM rust:1.99.1-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config ca-certificates && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml phpx.toml.example ./
COPY crates ./crates
RUN cargo build --release -p phpx-cli && cp target/release/phpx /usr/local/bin/phpx && phpx --help

# ---- runtime ----
FROM php:8.4-cli-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
  && useradd -u 10001 -m -s /usr/sbin/nologin phpx \
  && mkdir -p /app/examples /app/data /app/certs && chown -R phpx:phpx /app
COPY --from=builder /usr/local/bin/phpx /usr/local/bin/phpx
# Vanilla fixture for quickstart (read-only app code, writable data dir).
COPY examples/vanilla-php ./app/examples/vanilla-php
COPY phpx.toml.example /app/phpx.toml.example
# Default Docker config: listen on all interfaces, serve vanilla fixture.
RUN printf '%s\n' \
  'schema_version = 1' \
  '' \
  '[server]' \
  'host = "0.0.0.0"' \
  'port = 8080' \
  'tls = false' \
  'request_body_limit = "16MB"' \
  '' \
  '[application]' \
  'root = "/app/examples/vanilla-php"' \
  'document_root = "public"' \
  'entrypoint = "index.php"' \
  '' \
  '[runtime]' \
  'mode = "process"' \
  'php_version = "8.4"' \
  '' \
  '[runtime.process]' \
  'workers = 4' \
  'max_requests = 1000' \
  'request_timeout_seconds = 30' \
  'startup_timeout_seconds = 10' \
  'memory_limit = "256M"' \
  '' \
  '[runtime.embedded]' \
  'enabled = false' \
  '' \
  '[observability]' \
  'json_logs = true' \
  'metrics = true' \
  > /app/phpx.toml && chown phpx:phpx /app/phpx.toml /app/phpx.toml.example
WORKDIR /app
EXPOSE 8080
USER phpx
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
CMD ["phpx", "serve", "--config", "/app/phpx.toml"]
