# Persistent worker contract (P0-A defined, P0-B implemented)

Status: **Implemented.** `execution_mode = "persistent"` selects the fixed-size
supervised pool (`PersistentPool`); `execution_mode = "one-shot"` (default)
keeps the one-shot throttle. Both report via CLI/JSON and
`phpx_execution_mode_info`.

## 1. Modes

| Mode | Meaning | Status |
|------|---------|--------|
| `one-shot` | One fresh `php` process per admitted request (`execute_one_shot`). No cross-request state by construction. | Implemented (M1, hardened M4) |
| `persistent` | Fixed-size pool of long-lived supervised PHP workers with identity, handshake, recycling. | Deferred to P0-B; configuring it fails with an actionable error |

## 2. Identity

- `worker_id: u64` — stable slot index across restarts (see `WorkerIdentity` in `phpx-runtime`).
- `incarnation: u64` — bumps every time the OS process for a slot is (re)spawned.
- `pid: Option<u32>` — live OS process handle while supervised; `None` before spawn / after reaping.
- A logical recycle counter alone (today's `recycled`) is **not** a process recycle;
  P0-B must tie recycling to a new incarnation + fresh PID.

## 3. Transport and protocol

- Envelope: `PhpRequest` / `PhpResponse` JSON with `protocol_version` (current `1`, see `phpx-runtime::PROTOCOL_VERSION`).
- Framing: 4-byte big-endian length + JSON (`encode_frame` / `decode_frame` / `write_frame`).
  Prototype transport is stdio pipes; Unix-socket transport lands with P0-B.
- Breaking envelope changes bump `PROTOCOL_VERSION`; supervisor and worker must
  agree, else the request fails with a protocol error instead of executing.
- Blocking PHP execution **never** runs on Tokio executor threads: the gateway
  dispatches via `tokio::task::spawn_blocking` (ADR-0006). P0-B keeps this rule
  for handshake, I/O pump, and supervision waits.

## 4. Lifecycle (P0-B target, not yet code)

`Starting -> Ready -> Busy -> Ready`, plus `Ready -> Draining -> Stopped`,
`Busy -> Draining -> Stopped`, `Busy -> Unhealthy -> Restarting -> Ready`,
`Starting -> Failed`. Replacement becomes ready only after a successful
startup handshake inside `startup_timeout_seconds`.

## 5. Which applications can use persistent mode

None yet beyond fixtures — persistent mode is selectable since P0-B and
eligibility starts with stateless vanilla PHP (proven: reuse, crash recovery,
timeout, recycle, SAPI reset), then Composer-based apps (proven: SAPI reset
across reuse), then real Laravel / WordPress smoke suites (P0-D work, not
claimed). Applications
relying on cross-request in-memory state, `$_SESSION` without external storage,
`$_FILES` streaming, or `php://input` replay must stay on one-shot until the
state-reset rules below are proven for them.

## 6. State-reset limitations

A persistent PHP worker must not leak state between requests. Until proven
otherwise, assume leakage via: superglobals, static variables, opcache
per-process state, open handles, working directory, locale/timezone, RNG state,
and shutdown-function side effects. The reset contract (P0-B + P1-B, implemented in
`worker-persistent.php`): `$_GET`/`$_POST` (urlencoded parsed like the web SAPI;
multipart not parsed)/`$_COOKIE`/`$_REQUEST` rebuilt per
request, stale `HTTP_*` dropped, `PHPX_*` re-exported per request, output
buffering reset, `phpx_input()` served from the frame body. Static variables
and `$GLOBALS` set by app code intentionally persist (documented limit: use
one-shot when strict isolation is required).

## 7. Resource guardian (P0-C, implemented)

Per-worker OS RSS is sampled at two points: synchronously at request
completion, and by a background sampler thread every
`memory_sample_interval_seconds` for idle workers (Busy workers are never
blocked by the sampler). Observations per incarnation: `rss_bytes`,
`peak_rss_bytes` (max sampled, lower bound on the true peak), `healthy`,
`last_ok_at`, plus `startup_duration_ms` (spawn+handshake) and
`shutdown_duration_ms` (kill+wait) from lifecycle transitions.

Separate enforcement points (never conflate):

- `memory_limit` (PHP `-d memory_limit`): Zend heap only, both modes.
- `memory_soft_limit` (OS RSS): graceful recycle, replacement first.
- `memory_hard_limit` (OS RSS): protective termination + respawn.

Soft < hard when both set (`"0"` disables). Triggers are counted per worker
(`phpx_worker_memory_limit_hits_total{threshold}`) and restarts carry
`soft-limit`/`hard-limit` causes. Overshoot between sample points is possible
(sampling-based, not a hard cap); RSS != PHP heap, and neither stable nor
high RSS proves anything about leaks — the guardian recycles and protects,
it does not diagnose.
