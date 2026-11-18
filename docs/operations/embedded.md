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
- PHPX Redis driver stays deferred; app-native Redis via WordPress and Laravel keeps working without PHPX.


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

