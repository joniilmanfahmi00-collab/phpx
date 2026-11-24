# PHPX embedded runtime (M6 experimental)

Status: M6.1 stub DONE - capability reporting + single-entry domain + shared contract tests.
Native php-embed linking deferred (needs Linux + php-embed dev libs + ABI pin).

Process Runtime stays the active backend. Embedded is opt-in and never default.

## Capabilities

- available=false in this build (no native lib linked).
- single_threaded=true, max_concurrent_entries=1.
- nts_only=true (ZTS not claimed).
- Concurrent entry is rejected with an actionable error, never queued.

## Config

[runtime.embedded] enabled=false is the default. Set enabled=true plus runtime.mode=embedded to opt in (experimental).
Validation still rejects mode=embedded without enabled=true.

## CLI

- runtime list shows embedded status experimental-disabled or experimental-enabled plus available=false.
- runtime inspect includes an embedded object with enabled, available, single_threaded, max_concurrent_entries.
- Execution through embedded is not wired yet; requests keep using the process backend.

## Deferred (later M6)

- Native php-embed build and linking on Linux with pinned headers and ABI matrix.
- Narrow C ABI bridge with memory ownership and lifecycle rules.
- NTS and ZTS constraint tests plus crash containment tests.
- Laravel and WordPress compatibility subsets through embedded.
- PHPX Redis queue/cache drivers share the same contract suites as the
  memory/sqlite/filesystem drivers; app-native Redis via WordPress and
  Laravel keeps working without PHPX.


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

## Narrow bridge (M6 Point 3 DONE)

- Module bridge, BRIDGE_VERSION=1. Allowlist only: startup, execute_once, shutdown. Unknown ops rejected before envelope checks.
- C boundary headers: BridgeRequest plus BridgeResponse with repr C. Stub validates op plus envelope, then reports unlinked.

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
