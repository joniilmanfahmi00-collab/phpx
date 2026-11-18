# PHPX storage (M5 phase 3)

Status: phase 3 DONE — local filesystem real, S3 prototype (config real, transport deferred).
Sessions stay with conventional PHP sessions; this is object storage for app files.

SPEC 7.4: local filesystem first, S3-compatible later.

## Drivers

- `local`: files under `[storage] root` (e.g. `./data/storage`).
  Parents auto-created. Paths canonicalized + `starts_with` root check,
  traversal (`..`, absolute, NUL, empty segments) rejected with exit 2.
  `list` walks root, bounded max 1000, sorted. Good for single-node baseline.
- `s3`: prototype. Config (`endpoint` + `bucket` + `region`) validated real;
  transport deferred — every data op returns actionable
  `not implemented: s3 storage prototype deferred ... use driver=local`.
  `status` works (driver `s3-prototype`, 0 objects) so operators can validate config.
  Needs network + credentials + integration justification before real transport.

## Semantics (SPEC 7.4)

- Bounded: path 1-512 chars, segments `[A-Za-z0-9 . _ - @]`, `/` separator only,
  object max 4MB, list max 1000. Full/oversize = error (backpressure, never unbounded).
- Operations: `put` / `get` / `delete` / `list` / `status`.
- Durable ack: local `put` fsyncs via OS write; `status` counts from disk walk.
- Exactly-once NOT promised; consumers must be idempotent.

## CLI (phase 3 real)

- `phpx storage status [--json]` — driver, objects, bytes, root.
- `phpx storage put --path docs/hello.txt --value hi [--json]`
- `phpx storage get --path docs/hello.txt [--json]` — miss exit 0 + note.
- `phpx storage delete --path docs/hello.txt [--json]`
- `phpx storage list [--prefix docs/] [--json]` — bounded, sorted.
- S3 prototype: `status` exit 0, data ops exit 2 with deferred note.
- Bad path/object = exit 2 with actionable message.

## Config

```toml
[storage]
driver = "local" # local | s3 (prototype)
root = "./data/storage" # required when driver=local
# S3 prototype (driver=s3):
# endpoint = "https://s3.example"
# bucket = "phpx-dev"
# region = "auto"
```

Validation (`phpx-config`): unknown driver exits 2, local without `root`
exits 2, s3 without `endpoint` + `bucket` exits 2.

## Tests

- `crates/phpx-storage/tests/contract.rs` (local driver):
  put/get/list, survive-reopen, rejects traversal.
- `cargo test -p phpx-storage` green (2 unit + 3 contract).
- CLI: `storage status` exit 0, `--json` has `objects`.

## Deferred (later M5)

- Real S3 transport (SigV4, multipart, retries) with integration justification.
- Gateway-integrated storage middleware.
- Backup/restore runbook for durable roots.
