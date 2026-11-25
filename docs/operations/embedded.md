# PHPX embedded runtime (M6 experimental)

Status: M6 gates partially proven — ABI pin, execution domain, ownership,
containment DONE and unit-proven; extension audit live; isolation lifecycle,
engine lifecycle and gate assessor modeled; compat parity and stability HOLD
until a live engine executes. `available` is derived from the gates
(`gates::available()`, fail-closed) instead of a literal false.

Process Runtime stays the active backend. Embedded is opt-in and never default.

## Promotion gates (P2, assessor `gates::assess()`)

| Gate | Status | Evidence |
|---|---|---|
| `abi_pin` — supported PHP build + ABI matrix | DONE | `abi::AbiPin` pin + matrix tests (8.3/8.4/8.5 NTS non-debug accept; ZTS/Debug/older reject) |
| `native_link` — real PHP Embed SAPI link | HOLD (stub) / DONE when linked | `build.rs` links libphp only on Linux + `PHPX_EMBED_LINK=1`; WSL Debian gate 2026-11-19 green linked |
| `execution_domain` — controlled domain + thread constraints | DONE | Single-entry `EmbeddedDomain`, 8-thread race = exactly 1 winner, panic poisons gracefully |
| `ownership` — ownership/lifetime rules across the C ABI | DONE | 5 rules + `RequestScope` tracker, enforced by construction |
| `containment` — failure containment strategy | DONE | Validate-then-enter-then-cap order, exact 16 MiB cap, junk rejected before entry |
| `extensions` — extension compatibility tests | LIVE (machine-dependent) | `ext::ExtReport::probe()` audits live `php -m` vs the 11-extension doctor matrix; HOLD lists the missing extensions |
| `isolation` — request state isolation tests | HOLD | `IsolationScope` lifecycle modeled + unit-proven (clean seal vs leak fail-closed); live Zend executor reset per request unproven without a hosted engine |
| `compat_parity` — parity on the compatibility suite | HOLD | Subsets mirror M4 fixtures with per-subset blockers (table below); no subset executes through an engine yet |
| `stability` — stability evidence, never throughput alone | HOLD | No archived linked-engine soak (consecutive clean entries + RSS trend per the soak-long convention) |

`available()` is true only when every gate passes — the mechanical promotion
condition. `runtime inspect --json` exposes the full `gates` array; `phpx
doctor` keeps its informational php-embed checks.

## Capabilities

- available=false until every promotion gate passes (derived from
  `gates::available()`, fail-closed — not a literal).
- single_threaded=true, max_concurrent_entries=1.
- nts_only=true (ZTS not claimed).
- Concurrent entry is rejected with an actionable error, never queued.

## Config

[runtime.embedded] enabled=false is the default. Set enabled=true plus runtime.mode=embedded to opt in (experimental).
Validation still rejects mode=embedded without enabled=true.

## CLI

- runtime list shows embedded status experimental-disabled or experimental-enabled plus derived available/linked.
- runtime inspect includes an embedded object with enabled, available, single_threaded, max_concurrent_entries, abi pin, and the full `gates` array (per-gate pass + evidence).
- Execution through embedded is not wired yet; requests keep using the process backend.

## Engine lifecycle (P2 gate model, DONE)

Module `engine`: Cold → Initializing → Ready → Serving → Draining/Shutdown,
Failed terminal except Shutdown. Pure transition table, unit-tested. Hosting
a live engine stays closed until the gates above pass; the machine is the
contract the live engine will obey (only Ready enters, only Serving exits).

## Isolation lifecycle (P2 gate model, DONE)

Module `isolation`: one `IsolationScope` per request — track every byte
handed to the engine, release everything before the next entry, seal the
boundary. A seal over retained bytes fails closed naming the leak. The live
Zend executor reset plugs into this lifecycle; the gate stays HOLD until a
hosted engine proves per-request reset.

## Extension compatibility (P2 gate, LIVE)

Module `ext`: `REQUIRED_EXTENSIONS` (the 11-extension doctor matrix) +
`ExtReport::audit()` over `php -m` output + `ExtReport::probe()` for the
live listing. The assessor marks the gate DONE when the local `php -m`
satisfies the matrix and HOLD naming the missing extensions otherwise.

## Deferred (later M6)

- Native php-embed link in CI (gate `native_link`; manual WSL gate proven linked-green, CI stays stub until php-dev headers are available on the runner).
- Live C ABI bridge execution (`startup`/`execute_once`/`shutdown` against a hosted engine; `call_stub` keeps failing closed until then).
- Laravel and WordPress compatibility subsets through embedded (gate `compat_parity`; per-subset blockers in the table below).


## ABI pin (M6.2 DONE)

Single source: phpx-runtime-embedded abi module (AbiPin).

- Supported PHP: 8.3 / 8.4 / 8.5 (prefix match, so 8.4.12 matches 8.4).
- SAPI: embed only. Headers: sapi/embed/php_embed.h, main/php.h, Zend/zend_API.h.
- Linux libs: libphp.so, libphp8.so (pkg php-embed on Debian and Ubuntu php-dev installs).
- NTS only, non-debug only (PHP_DEBUG=0). ZTS and Debug builds are rejected with actionable errors.
- Dev machine note: Windows PHP 8.3.30 here is ZTS, so AbiPin check rejects it by design. Process Runtime stays active.
- CLI JSON: runtime list and runtime inspect now include embedded.abi from AbiPin pinned values.


## Link probe (M6 Point 1 DONE)

- Module link: LinkProbe probes php-config plus php at runtime, never fails the build. found stays false until a Linux NTS php-embed toolchain is wired.
- build_plan: Linux x86_64 with php-dev NTS non-debug 8.3-8.5, php-config includes plus libs, build.rs link step, narrow bridge calls, contract tests gate every claim.
- Native link deferred: Windows dev box has no php-embed dev libs, so probe reports actionable steps instead of linking.

## Narrow bridge (M6 Point 3 DONE) + live-eval shim (P2)

- Module bridge, BRIDGE_VERSION=1. Allowlist only: startup, execute_once, shutdown. Unknown ops rejected before envelope checks.
- C boundary headers: BridgeRequest plus BridgeResponse with repr C. Stub validates op plus envelope, then reports unlinked.
- `src/engine_shim.c` (linked builds only): ub_write capture with a 16 MiB cap, `zend_first_try/catch` around every eval so a fatal longjmps back instead of killing the host, per-eval output handoff.
- Eval semantics learned from zend_execute_API.c: **when retval is requested the engine wraps the snippet as `return <code>;` itself** — statements go through with retval NULL (`execute_once`: `echo 'x';`, assignments) while bare expressions go with retval (`eval_expr`: `6*7`, not `return 6*7;`). Mixing them up is a clean parse error, never a crash (pinned by the linked tests).

## Ownership and lifecycle (M6 Point 4 DONE)

- Module ownership, five rules: Rust owns buffers, no pointers cross requests, engine handles live one request, concurrent entry rejected before allocation, oversize rejected before allocation.
- RequestScope tracks bytes per request and Drop closes the scope.

## NTS and ZTS matrix (M6 Point 6 DONE)

- NTS only, non-debug only. ZTS plus Debug rejected with actionable errors.

| PHP | NTS non-debug | ZTS | Debug | Status |
|-----|---------------|-----|-------|--------|
| 8.3 | ok | rejected | rejected | pin active, link deferred |
| 8.4 | ok | rejected | rejected | pin active, link deferred |
| 8.5 | ok | rejected | rejected | pin active, link deferred |
| 8.2 and older | rejected | rejected | rejected | unsupported |

## Compat subsets (M6 Point 9 DONE)

- Module compat mirrors M4 fixtures: vanilla, laravel-minimal, wordpress-minimal, woocommerce-minimal.
- run_subset_stub validates subset id plus envelope, then reports deferred. Same subsets stay green through process runtime today.

## Containment and limits (M6 Point 10 DONE + linked-mode gate 2026-11-19)

Gate run (WSL Debian, php-config 8.3.36 NTS, PHPX_EMBED_LINK=1, libphp linked):
`cargo test -p phpx-runtime-embedded` = 27 lib + 12 contract, all green WITH libphp
linked. Proves the gates hold in linked mode, not only in stub mode:

- Panic inside the domain poisons the mutex, gateway thread survives, next
  entry reports graceful `poisoned` error (no hang, no crash).
- Body exactly 16 MiB passes the size check (no off-by-one); 16 MiB + 1 is
  rejected naming MAX_FRAME_BYTES.
- Junk envelope is rejected BEFORE domain entry (single-entry domain cannot
  be exhausted by malformed frames).
- Clean guard release allows re-entry (no deadlock).
- 8-thread race still yields exactly 1 winner under linked build.
- `execute_guarded` hold message is link-aware: linked=true reports
  "engine entry closed until M6 gates pass", stub reports "not linked".

## Containment and limits (M6 Point 10 DONE)

- Module containment, ResourceLimits standard: max_frame_bytes 16 MiB, max_wall_ms 30000, single_entry true.
- Order: validate envelope, enter single-entry domain, enforce size cap, then report unlinked. Gateway never hosts a live engine in this build, so a crash cannot take it down.
- Stress proof: 8 threads racing try_enter yields exactly 1 winner.

## Supported build matrix (M6 exit DONE)

| OS | PHP | Thread safety | Status |
|----|-----|---------------|--------|
| Linux x86_64 | 8.3-8.5 NTS non-debug | NTS | target for future native link, deferred in v0.1 |
| Linux x86_64 | ZTS or Debug | ZTS or Debug | rejected by AbiPin |
| Windows x64 | 8.3.30 ZTS dev box | ZTS | rejected by AbiPin, process runtime active |
| Docker bookworm | php:8.4 NTS CLI | NTS CLI | process runtime only, embed deferred |

## Linked gate run (WSL Debian, 2026-11-24, P2 gates)

- Toolchain: php-config 8.3.36 NTS + `/usr/include/php/20230831/sapi/embed/php_embed.h` + `libphp.so`; build log: `linked php-embed (php-config 8.3.36, libs=["php8.3", ...])`.
- `PHPX_EMBED_LINK=1 cargo test -p phpx-runtime-embedded` = **42 lib + 16 contract + 8 linked, all green WITH libphp linked** (was 28 + 12 on 2026-11-19).
- First live executions through the narrow bridge: boot, `6*7 = 42`, echo output capture, shared-executor leak pinned, request-cycle isolation (`set → cycle → clean`), vanilla fixture parity (`hello from phpx vanilla|rid=direct`), bailout containment + engine reuse, bailout → cycle recovery — all green, default parallel test threads (single dedicated engine thread; touching the engine from different OS threads trips Zend stack tracking after a bailout).
- Live `gates::assess()` on the linked build: **7 PASS + 2 HOLD, `available=false`** (fail-closed, correct):
  - PASS: `abi_pin`, `native_link`, `execution_domain`, `ownership`, `containment`, `extensions` (live `php -m` 11/11 on the NTS build), `isolation` (request-cycle reset proven live).
  - HOLD: `compat_parity` (1/4 — vanilla live, rest need SAPI request plumbing), `stability` (no archived linked soak).
- Promotion flips mechanically when the two HOLDs turn PASS with archived evidence; no code path changes needed (`available()` derives from the assessor).

## Live execution (P2 milestone, WSL-proven)

`tests/linked_engine.rs` (linked builds only) drives the real engine through
the narrow bridge — the first live PHPX embedded executions:

- boot: engine inits once per process (`phpx_embed_startup` idempotent).
- expression eval: `eval_expr("6*7")` returns long `42`.
- output capture: `execute_once("echo 'hello-live';")` returns the bytes
  (ub_write override targets the global `sapi_module` copy — overriding
  `php_embed_module` post-startup is a silent no-op because `sapi_startup()`
  copies the struct).
- shared-executor reality without a cycle: sequential evals share one
  executor, so `$GLOBALS` set in eval N is visible in eval N+1 by design
  (pinned `leak`).
- request cycle (`phpx_embed_next_request`, balanced RINIT pair): shutdown +
  startup hands back a fresh executor — `set → cycle → probe` reads `clean`
  and stays clean across bailouts (`bailout → cycle → clean`). This is the
  per-request isolation primitive the isolation gate is built on.
  NEVER add an extra `request_shutdown` before `php_embed_shutdown`: it
  already performs exactly one RSHUTDOWN, and the unbalanced extra one
  corrupts the heap at teardown (observed in the C probe, pinned as a rule
  in the shim).
- FFI contract learned from `zend_execute_API.c`: with retval requested the
  engine wraps the snippet as `return <code>;` itself — statements go
  retval-NULL, bare expressions go with retval. Mixing them is a clean
  parse error, never memory unsafety.
- bailout containment: `live_bailout_contained_host_survives` pins that an
  uncaught exception longjmps back to status 1 with the host alive and the
  engine reusable on the next eval (`Fatal error: Uncaught Exception` printed,
  process survives, `7*6` returns `42` right after).
- threading rule (proven the hard way): the NTS engine is pinned to ONE
  dedicated OS thread for the whole process (`tests/linked_engine.rs`
  routes every eval through a single engine thread). Parallel cargo tests
  that touched the engine from different threads tripped Zend stack
  tracking after a bailout (spurious "Maximum call stack size" on other
  threads' stacks) — serial passes, parallel failed, dispatcher fixed it.
  This mirrors the production constraint (single-entry domain) and the
  future server integration shape: one engine thread, never the gateway
  workers' threads.

The isolation + compat_parity + stability gates stay HOLD until per-request
reset, subset parity, and an archived soak land — `available()` remains
fail-closed.

## WSL gate verification (M6 gates, 2026-11-19, WSL Debian)

- G1 NTS: php -i shows Thread Safety disabled, Debug Build no, Zend Extension Build API420230831,NTS. PASS (NTS non-debug).
- G2 toolchain (after sudo apt-get install -y php8.3-dev libphp8.3-embed):
  - php-config --version => 8.3.36
  - /usr/include/php/20230831/sapi/embed/php_embed.h present
  - ldconfig -p | grep libphp => libphp.so => /lib/libphp.so. PASS.
- G3 contracts: cargo test -p phpx-runtime-embedded => 12 passed, 0 failed on Linux. PASS.
- G4 CLI:
  - phpx runtime list => process active (workers=4), embedded experimental-disabled (available=false, linked=false, single-threaded, max_entries=1); php target 8.4 | actual PHP 8.3.36 NTS.
  - phpx runtime inspect --json => embedded available false, enabled false, single_threaded true, max_concurrent_entries 1, abi sapi embed nts_only true. PASS (honest stub, no overclaim).

## Opt-in native link (experimental, never default)

- Default build stays stub: cargo build -p phpx-runtime-embedded => linked=false, available=false. Windows and ZTS boxes stay green.
- Linux NTS only:
  - sudo apt-get install -y php8.3-dev libphp8.3-embed (php8.4: php8.4-dev libphp8.4-embed)
  - PHPX_EMBED_LINK=1 cargo build -p phpx-runtime-embedded
  - PHPX_EMBED_LINK=1 cargo test -p phpx-runtime-embedded
- build.rs never fails the build. On missing toolchain, version mismatch, or non-linux TARGET it emits cargo:warning and stays stub.
- Even when linked=true, available stays false in v0.1 until compat, isolation, and stability gates pass. Docker stays process-runtime-only with embed deferred.
- OTel stays OTel-ready (traceparent passthrough + JSON logs), not OTLP exporter. cgroup and OS limits stay operator duty (Docker --cpus/--memory, systemd MemoryMax/CPUQuota).

## Opsi-2 compat explicit (2026-11-19, DONE)

No vague deferral: each embedded subset now names its green process fixture
AND its exact M6 blocker (compat.rs Subset.process_fixture + embedded_blocker).

| Subset | Process fixture (green today) | Embedded blocker (M6 gate) |
|--------|-------------------------------|----------------------------|
| vanilla | examples/vanilla-php (m4_compat: isolation + crash-recovery green) | engine entry closed: isolated Zend request lifecycle not wired (M6 isolation gate) |
| laravel-minimal | examples/laravel-minimal (m4_compat: routing + JSON green) | engine entry closed: framework bootstrap needs persistent opcache + env lifecycle (M6 stability gate) |
| wordpress-minimal | examples/wordpress-minimal (m4_compat: loop + search + shortcode green) | engine entry closed: WP_SIM global-state lifecycle per request not wired (M6 isolation gate) |
| woocommerce-minimal | examples/woocommerce-minimal (m4_compat: cart + checkout JSON green) | engine entry closed: after WP baseline; session/cart state lifecycle not wired (M6 isolation gate) |

Link-aware deferral: run_subset_stub keeps substring 'deferred' in both modes
so contract stays stable. linked=true reports 'engine entry closed until M6
gates pass', stub reports 'not linked'. Same subset stays green through
process runtime.

Gate runs 2026-11-19:
- Windows (ZTS dev box, stub): cargo test -p phpx-runtime-embedded = 28 lib + 12 contract, all green.
- WSL Debian (php-config 8.3.36 NTS, PHPX_EMBED_LINK=1, libphp linked): cargo test -p phpx-runtime-embedded = 28 lib + 12 contract, all green.
- WSL process compat: cargo test -p phpx-server --test m4_compat = 5 passed (vanilla, composer, laravel, wp+woo, metrics).
- runtime inspect (Windows): embedded linked=false, available=false, enabled=false. Honest stub, no overclaim.
