# PHPX M6 notes - Embedded Runtime experimental track

Goal: validate a second runtime backend without weakening the Process Runtime.
Status: M6.1 stub DONE (capability reporting + single-entry domain + shared contract tests).
Native php-embed linking deferred to later M6 (needs Linux + php-embed dev libs + ABI pin).

## M6.1 stub (DONE)

- New crate phpx-runtime-embedded: EmbeddedCapabilities report, EmbeddedDomain single-entry guard, execute_stub.
- Single-entry rule: at most one PHP engine entry at a time; concurrent entry rejected, never queued.
- Stub never executes PHP: validates envelope first, then returns an actionable unavailable error.
- Config gate unchanged: mode=embedded still requires [runtime.embedded] enabled=true (experimental).
- CLI real: runtime list shows experimental-disabled or experimental-enabled plus available=false.
- CLI real: runtime inspect includes an embedded object with enabled, available, single_threaded, max_concurrent_entries.
- Tests: 4 unit + 4 contract green (capability, single-entry, envelope roundtrip, stub-never-executes).
- Doc: docs/operations/embedded.md. Example stays process-first; embedded opt-in only.

## What changed vs M5

- Cargo workspace plus phpx-runtime-embedded member and dependency.
- phpx-cli depends on phpx-runtime-embedded for list and inspect output.
- roadmap.md: 5 M6 boxes checked with M6.1 stub notes; native linking boxes stay open.
- overview.md: M6.1 scope line plus Future updated to observability only.

## Deferred (later M6)

- Native php-embed build and linking with pinned headers and ABI matrix.
- Narrow C ABI bridge with memory ownership and lifecycle rules.
- NTS and ZTS constraint tests plus crash containment tests.
- Laravel and WordPress compatibility subsets through embedded.

Traceability: SPEC 3.1 second backend covered as experimental stub by M6.1;
SPEC 15 embedded tail partially covered (opt-in, capabilities, contracts); native execution still open.


## M6.2 ABI pin (DONE)

- New module phpx-runtime-embedded abi: AbiPin pinned values, supports_version prefix match, check with actionable errors, describe for humans.
- Pin: PHP 8.3/8.4/8.5, SAPI embed, headers sapi/embed/php_embed.h + main/php.h + Zend/zend_API.h, Linux libs libphp.so + libphp8.so, pkg php-embed, NTS only, non-debug only.
- CLI: runtime list --json and runtime inspect --json now include embedded.abi from AbiPin. Human output unchanged.
- Tests: 4 unit in abi module + 1 new contract test abi_pin_contract. Windows dev PHP 8.3.30 ZTS is rejected by check by design.
- Native link still deferred: needs Linux + php-embed dev libs. Pin is code, not link.


## M6 full stub (DONE)

All M6 work items are done as code plus tests plus docs, with the native
php-embed link honestly deferred to real Linux dev libs.

- Point 1 link probe: module link with LinkProbe plus build_plan. found stays false, tried list plus how_to_enable are actionable.
- Point 2 ABI pin: module abi with AbiPin, done in M6.2, now extended with matrix contract tests.
- Point 3 narrow bridge: module bridge, 3 allowed ops, repr C headers, stub validates op plus envelope then reports unlinked.
- Point 4 ownership: module ownership, 5 rules plus RequestScope tracker.
- Domain: EmbeddedDomain single-entry guard from M6.1, plus 8-thread stress test proving exactly 1 winner.
- Point 6 NTS and ZTS: matrix contract tests over 8.3 through 8.5 plus older versions; ZTS plus Debug always rejected.
- Capability reporting plus shared contracts: from M6.1, CLI JSON now includes embedded.abi.
- Point 9 compat: module compat with 4 subsets mirroring M4 fixtures; execution deferred, process stays green.
- Point 10 containment: module containment with standard limits plus guarded order; gateway never hosts a live engine.
- Exit: opt-in experimental yes, contracts green yes, no arbitrary concurrent entry proven by stress test, matrix plus limitations published in embedded.md plus limitations.md, process fallback stays default.

