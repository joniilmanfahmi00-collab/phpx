# Runtime embedded PHPX (M6 eksperimental)

Status: gerbang M6 terbukti — pin ABI, domain eksekusi, ownership,
containment DONE dan terbukti unit; audit ekstensi live; isolasi terbukti
live; kesetaraan kompatibilitas terbukti live; stabilitas terbukti via soak
terarsip; jalur serve P2 terkabel ke gateway. `available` diturunkan dari
gerbang (`gates::available()`, fail-closed), bukan literal false.

Process Runtime tetap backend aktif. Embedded opt-in dan tidak pernah default.

## Gerbang promosi (P2, asesor `gates::assess()`)

| Gerbang | Status | Bukti |
|---|---|---|
| `abi_pin` — build PHP + matriks ABI yang didukung | DONE | Pin `abi::AbiPin` + test matriks (8.3/8.4/8.5 NTS non-debug diterima; ZTS/Debug/lama ditolak) |
| `native_link` — link SAPI PHP Embed asli | DONE bila linked (HOLD bila stub) | `build.rs` me-link libphp hanya di Linux + `PHPX_EMBED_LINK=1`; gerbang WSL Debian hijau linked (2026-11-19 dan 2026-11-25) |
| `execution_domain` — domain terkendali + batasan thread | DONE | `EmbeddedDomain` entri-tunggal, race 8-thread = tepat 1 pemenang, panic meracuni graceful; engine live di-pin ke satu thread khusus |
| `ownership` — aturan ownership/lifetime lintas C ABI | DONE | 5 aturan + pelacak `RequestScope`, ditegakkan secara konstruksi |
| `containment` — strategi containment kegagalan | DONE | Urutan validasi-lalu-masuk-lalu-batasi, cap persis 16 MiB, sampah ditolak sebelum masuk; bailout live terkandung + engine dapat dipakai lagi |
| `extensions` — test kompatibilitas ekstensi | LIVE (tergantung mesin) | `ext::ExtReport::probe()` mengaudit `php -m` live vs matriks doctor 11-ekstensi; HOLD menyebut ekstensi yang hilang |
| `isolation` — test isolasi state request | DONE bila linked (HOLD bila stub) | Siklus RINIT `phpx_embed_next_request`: `set → cycle → probe` terbaca bersih, `bailout → cycle → bersih`; `exit` tunggal + RSHUTDOWN ganda merusak heap (aturan dipin) |
| `compat_parity` — kesetaraan dengan suite kompatibilitas | DONE bila linked (HOLD bila stub) | Keempat sim berjalan live via `live::serve`: vanilla hello-world, laravel routing+JSON+urlencoded, wordpress loop/search/shortcode+cookies, woocommerce cart/checkout+cookies (`exit` terkandung → mapping status terdokumentasi) |
| `stability` — bukti stabilitas, bukan throughput semata | DONE bila linked + soak terarsip (HOLD bila tidak) | `tests/linked_soak.rs`: 300 eval campuran + siklus request, 6 bailout terkandung, 0 gagal, RSS +278 KB steady-state (cap 10 MB); arsip dibaca balik oleh gerbang |

`available()` true hanya bila semua gerbang lolos — syarat promosi mekanis.
`runtime inspect --json` mengekspos array `gates` penuh; `phpx doctor`
mempertahankan cek php-embed informasionalnya.

## Kemampuan

- available=false hingga semua gerbang promosi lolos (diturunkan dari
  `gates::available()`, fail-closed — bukan literal).
- single_threaded=true, max_concurrent_entries=1. Gateway menyerikan lewat
  satu slot engine: request ekstra menunggu di antrean terbatas
  (`queue_bound` + `queue_wait_timeout_seconds` eksplisit, backpressure 503
  saat penuh/kedaluwarsa-tunggu — kontrak sama dengan pool process).
- nts_only=true (ZTS tidak diklaim).
- Timeout eksekusi eksplisit (`request_timeout_seconds`, 504 saat kedaluwarsa).
  Asimetri vs pool process (batas Beta yang didokumentasikan): gateway
  melaporkan 504 dalam budget tetapi tidak bisa membunuh NTS di tengah
  request — request panjang jalan hingga selesai dan request berikutnya
  antre di belakangnya.

## Konfig

[runtime.embedded] enabled=false adalah default. Set enabled=true plus runtime.mode=embedded untuk opt-in (eksperimental).
Validasi tetap menolak mode=embedded tanpa enabled=true. Semua bound eksplisit
(serde default menjaga konfig lama tetap terparse):

- `request_timeout_seconds` (default 30, 1-600): budget eksekusi sisi-gateway,
  504 saat kedaluwarsa (lihat asimetri timeout di atas).
- `queue_bound` (default auto = 16, 1-100000): antrean tunggu terbatas, 503 saat penuh.
- `queue_wait_timeout_seconds` (default 2, 0-120): tunggu terbatas untuk slot
  engine tunggal, 503 saat kedaluwarsa.
- `shutdown_drain_timeout_seconds` (default 10, 1-120): budget drain graceful.

## CLI

- runtime list menampilkan status embedded experimental-disabled atau experimental-enabled plus available/linked turunan.
- runtime inspect menyertakan objek embedded berisi enabled, available, single_threaded, max_concurrent_entries, pin abi, dan array `gates` penuh (lolos + bukti per gerbang). Bila mode=embedded, field pool (execution_mode, workers, bound, timeout) melaporkan pool embedded, bukan angka process.
- `runtime.mode=embedded` terkabel: gateway membangun `EmbeddedPool`
  single-flight (pooling + routing + timeout eksplisit di atas) dan melayani
  rute yang sama (direct .php / statik / front-controller) melalui
  `live::serve_with_timeout`. Build stub fail-closed per request
  (500, "live engine unavailable" yang actionable — tidak pernah hang); build
  linked mengeksekusi live (lihat `crates/phpx-server/tests/p2_embedded_gateway.rs`).

## Siklus hidup engine (model gerbang P2, DONE)

Modul `engine`: Cold → Initializing → Ready → Serving → Draining/Shutdown,
Failed terminal kecuali Shutdown. Tabel transisi murni, unit-test. Menghosting
engine live tetap tertutup hingga gerbang di atas lolos; mesin inilah kontrak
yang akan dipatuhi engine live (hanya Ready yang masuk, hanya Serving yang keluar).

## Siklus hidup isolasi (model gerbang P2, DONE — terbukti live)

Modul `isolation`: satu `IsolationScope` per request — lacak setiap byte yang
diserahkan ke engine, lepas semuanya sebelum masuk berikutnya, segel batasnya.
Segel di atas byte yang tertahan fail-closed dengan menyebut bocornya. Reset
eksekutor Zend live terpasang ke lifecycle ini; reset per-request terbukti
via siklus RINIT live (`set → cycle → bersih`).

## Kompatibilitas ekstensi (gerbang P2, LIVE)

Modul `ext`: `REQUIRED_EXTENSIONS` (matriks doctor 11-ekstensi) +
`ExtReport::audit()` atas output `php -m` + `ExtReport::probe()` untuk listing
live. Asesor menandai gerbang DONE bila `php -m` lokal memenuhi matriks dan
HOLD dengan menyebut ekstensi yang hilang bila tidak.

## Ditunda (M6 lanjutan)

- Link php-embed nativ di CI (gerbang `native_link`; gerbang WSL manual terbukti
  hijau linked, CI tetap stub hingga header php-dev tersedia di runner).
- Klaim produksi apa pun (armada, multi-node, SLO) — tetap di luar Beta.

## Pin ABI (M6.2 DONE)

Sumber tunggal: modul abi phpx-runtime-embedded (AbiPin).

- PHP yang didukung: 8.3 / 8.4 / 8.5 (prefix match, jadi 8.4.12 cocok dengan 8.4).
- SAPI: embed saja. Header: sapi/embed/php_embed.h, main/php.h, Zend/zend_API.h.
- Lib Linux: libphp.so, libphp8.so (pkg php-embed pada instalasi php-dev Debian dan Ubuntu).
- Hanya NTS, hanya non-debug (PHP_DEBUG=0). Build ZTS dan Debug ditolak dengan error actionable.
- Catatan mesin dev: PHP 8.3.30 Windows di sini ZTS, jadi cek AbiPin menolaknya by design. Process Runtime tetap aktif.
- CLI JSON: runtime list dan runtime inspect kini menyertakan embedded.abi dari nilai pin AbiPin.

## Probe link (M6 Poin 1 DONE)

- Modul link: LinkProbe mem-probe php-config plus php saat runtime, tidak pernah menggagalkan build. found tetap false hingga toolchain php-embed NTS Linux dikabel.
- build_plan: Linux x86_64 dengan php-dev NTS non-debug 8.3-8.5, include php-config plus lib, tahap link build.rs, pemanggilan narrow bridge, test kontrak menjaga setiap klaim.
- Link nativ ditunda: box dev Windows tidak punya lib dev php-embed, jadi probe melaporkan langkah actionable alih-alih me-link.

## Narrow bridge (M6 Poin 3 DONE) + shim live-eval (P2)

- Modul bridge, BRIDGE_VERSION=1. Hanya allowlist: startup, execute_once, shutdown. Op tak dikenal ditolak sebelum cek envelope.
- Header batas C: BridgeRequest plus BridgeResponse dengan repr C. Stub memvalidasi op plus envelope, lalu melaporkan unlinked.
- `src/engine_shim.c` (hanya build linked): capture ub_write dengan cap 16 MiB, `zend_first_try/catch` di sekitar setiap eval agar fatal longjmp balik alih-alih membunuh host, handoff output per-eval.
- Semantik eval dari zend_execute_API.c: **bila retval diminta, engine membungkus snippet sebagai `return <code>;` sendiri** — statement lewat dengan retval NULL (`execute_once`: `echo 'x';`, assignment) sedangkan ekspresi telanjang lewat dengan retval (`eval_expr`: `6*7`, bukan `return 6*7;`). Tertukar berujung parse error bersih, bukan crash (dipin oleh test linked).

## Ownership dan lifecycle (M6 Poin 4 DONE)

- Modul ownership, lima aturan: Rust memiliki buffer, tidak ada pointer lintas request, handle engine hidup satu request, entri konkuren ditolak sebelum alokasi, oversize ditolak sebelum alokasi.
- RequestScope melacak byte per request dan Drop menutup scope.

## Matriks NTS dan ZTS (M6 Poin 6 DONE)

- Hanya NTS, hanya non-debug. ZTS plus Debug ditolak dengan error actionable.

| PHP | NTS non-debug | ZTS | Debug | Status |
|-----|---------------|-----|-------|--------|
| 8.3 | ok | ditolak | ditolak | pin aktif, terbukti linked via gerbang manual |
| 8.4 | ok | ditolak | ditolak | pin aktif |
| 8.5 | ok | ditolak | ditolak | pin aktif |
| 8.2 dan lebih tua | ditolak | ditolak | ditolak | tidak didukung |

## Subset kompatibilitas (M6 Poin 9 DONE — live via serve path)

- Modul compat mencerminkan fixture M4: vanilla, laravel-minimal, wordpress-minimal, woocommerce-minimal.
- Keempat subset berjalan LIVE via `live::serve` pada build linked (dipin oleh `linked_parity`); stub melaporkan deferred. Subset yang sama tetap hijau lewat process runtime.

## Containment dan batas (M6 Poin 10 DONE + gerbang linked-mode 2026-11-19)

Run gerbang (WSL Debian, php-config 8.3.36 NTS, PHPX_EMBED_LINK=1, libphp ter-link):
`cargo test -p phpx-runtime-embedded` = 27 lib + 12 kontrak, semua hijau DENGAN libphp
ter-link. Membuktikan gerbang bertahan dalam mode linked, bukan hanya mode stub
(rekaman historis; run 2026-11-24/25 di bawah ini yang kini berlaku):

- Panic di dalam domain meracuni mutex, thread gateway selamat, entri berikut
  melaporkan error `poisoned` yang graceful (tanpa hang, tanpa crash).
- Body tepat 16 MiB lolos cek ukuran (tanpa off-by-one); 16 MiB + 1 ditolak
  dengan menyebut MAX_FRAME_BYTES.
- Envelope sampah ditolak SEBELUM masuk domain (domain entri-tunggal tidak
  bisa dihabiskan oleh frame malformed).
- Rilis guard yang bersih memungkinkan masuk ulang (tanpa deadlock).
- Race 8-thread tetap menghasilkan tepat 1 pemenang pada build linked.

## Containment dan batas (M6 Poin 10 DONE)

- Modul containment, ResourceLimits standar: max_frame_bytes 16 MiB, max_wall_ms 30000, single_entry true.
- Urutan: validasi envelope, masuk domain entri-tunggal, tegakkan cap ukuran. Gerbang tidak pernah menghosting engine live dalam build stub, jadi crash tidak bisa menjatuhkannya.
- Bukti stres: 8 thread berlomba try_enter menghasilkan tepat 1 pemenang.

## Matriks build yang didukung (M6 exit DONE)

| OS | PHP | Thread safety | Status |
|----|-----|---------------|--------|
| Linux x86_64 | 8.3-8.5 NTS non-debug | NTS | terbukti linked via gerbang manual (CI tetap stub) |
| Linux x86_64 | ZTS atau Debug | ZTS atau Debug | ditolak oleh AbiPin |
| Windows x64 | 8.3.30 ZTS dev box | ZTS | ditolak oleh AbiPin, process runtime aktif |
| Docker bookworm | php:8.4 NTS CLI | NTS CLI | hanya process runtime |

## Run gerbang linked (WSL Debian, 2026-11-24/25, gerbang P2)

- Toolchain: php-config 8.3.36 NTS + `/usr/include/php/20230831/sapi/embed/php_embed.h` + `libphp.so`; log build: `linked php-embed (php-config 8.3.36, libs=["php8.3", ...])`.
- `PHPX_EMBED_LINK=1 cargo test -p phpx-runtime-embedded` = **42 lib + 16 kontrak + 8 linked-engine + 3 linked-parity + 1 linked soak, semua hijau DENGAN libphp ter-link** (dulu 28 + 12 pada 2026-11-19; build stub menjalankan 42 + 17 dengan test khusus-linked dikompilasi keluar).
- Eksekusi live pertama lewat narrow bridge: boot, `6*7 = 42`, capture output echo, leak shared-executor dipin, isolasi siklus-request (`set → cycle → bersih`), paritas fixture vanilla (`hello from phpx vanilla|rid=direct`), containment bailout + reuse engine, recovery bailout → cycle — semua hijau, thread test paralel default (satu thread engine khusus; menyentuh engine dari thread OS berbeda merusak pelacakan stack Zend setelah bailout).
- Rekaman soak (`data/bench/soak-embed-latest/SUMMARY.json`, di-gitignore): 300 eval campuran + 30 siklus request, 307 bersih, 6 bailout terkandung, 0 gagal, RSS +278 KB steady-state (cap 10 MB, pasca-warmup) dalam ~1,3 dtk — gerbang stabilitas membaca arsip ini balik (test menegaskan flip-nya end to end).
- `gates::assess()` live pada build linked: **9 PASS, `available=true`**. Gerbang jujur: build stub melaporkan HOLD + `available=false`; build linked tanpa arsip soak melaporkan 8/9 + `available=false`.
  - PASS: `abi_pin`, `native_link`, `execution_domain`, `ownership`, `containment`, `extensions` (live `php -m` 11/11 pada build NTS), `isolation` (reset siklus-request terbukti live), `stability` (arsip soak valid), `compat_parity` (keempat sim live: vanilla hello-world, laravel routing+JSON termasuk `exit` terkandung, wordpress loop/search/shortcode, woocommerce cart/checkout termasuk `exit` terkandung).
- Arti `available=true`: runtime terbukti mampu (request terisolasi, kegagalan terkandung, stabil di bawah soak, paritas app pada suite sim). Gateway default ke `process`; pilih `runtime.mode=embedded` (+ `enabled=true`) untuk melayani lewat pool embedded single-flight — pooling, routing dan timeout eksplisit terkabel (P2 serve path DONE, `p2_embedded_gateway.rs` hijau di stub dan linked). Tidak ada perubahan kode yang dibutuhkan untuk flip itu sendiri — `available()` diturunkan dari asesor.

## Eksekusi live (milestone P2, terbukti WSL)

`tests/linked_engine.rs` (hanya build linked) menggerakkan engine asli lewat
narrow bridge — eksekusi embedded live PHPX pertama:

- boot: engine init sekali per proses (`phpx_embed_startup` idempoten).
- eval ekspresi: `eval_expr("6*7")` mengembalikan long `42`.
- capture output: `execute_once("echo 'hello-live';")` mengembalikan byte-nya
  (override ub_write menarget salinan `sapi_module` global — meng-override
  `php_embed_module` pasca-startup adalah no-op diam karena `sapi_startup()`
  menyalin struct).
- realitas shared-executor tanpa siklus: eval berurutan berbagi satu eksekutor,
  jadi `$GLOBALS` yang di-set di eval N terlihat di eval N+1 by design
  (dipin `leak`).
- siklus request (`phpx_embed_next_request`, pasangan RINIT seimbang): shutdown +
  startup menyerahkan eksekutor fresh — `set → cycle → probe` terbaca `clean`
  dan tetap bersih lintas bailout (`bailout → cycle → clean`). Inilah primitif
  isolasi per-request tempat gerbang isolasi dibangun.
  JANGAN pernah tambah `request_shutdown` ekstra sebelum `php_embed_shutdown`: ia
  sudah melakukan tepat satu RSHUTDOWN, dan tambahan yang tak seimbang merusak
  heap saat teardown (teramati di probe C, dipin sebagai aturan di shim).
- Kontrak FFI dari `zend_execute_API.c`: dengan retval diminta, engine
  membungkus snippet sebagai `return <code>;` sendiri — statement lewat
  retval-NULL, ekspresi telanjang lewat dengan retval. Tertukar berujung parse
  error bersih, bukan memory unsafety.
- containment bailout: `live_bailout_contained_host_survives` mem-pin bahwa
  exception tak tertangkap longjmp balik ke status 1 dengan host hidup dan
  engine dapat dipakai lagi pada eval berikut (`Fatal error: Uncaught Exception`
  tercetak, proses selamat, `7*6` mengembalikan `42` tepat setelahnya), plus
  `live_bailout_then_cycle_recovers_cleanly` (siklus setelah fatal).
- soak stabilitas: `tests/linked_soak.rs` menjalankan 300 eval campuran (ekspresi +
  echo + siklus tiap 10 + bailout terkandung dengan reuse segera tiap 50 +
  include paritas vanilla final), menegaskan 0 kegagalan tak terduga dan
  pertumbuhan RSS dalam cap, mengarsipkan `SUMMARY.json` + `rss.csv` di bawah
  `data/bench/soak-embed-<ts>/` yang di-gitignore plus salinan
  `soak-embed-latest` yang dibaca balik gerbang stabilitas (test menegaskan
  flip-nya end to end).
  `PHPX_EMBED_SOAK_DIR` meng-override direktori arsip; gerbang juga menerima
  `PHPX_EMBED_SOAK_RECORD` untuk filenya.
- aturan threading (terbukti dengan cara keras): engine NTS di-pin ke SATU
  thread OS khusus untuk seluruh proses (`tests/linked_engine.rs` merutekan
  setiap eval lewat satu thread engine). Test cargo paralel yang menyentuh
  engine dari thread berbeda menjegal pelacakan stack Zend setelah bailout
  ("Maximum call stack size" palsu di stack thread lain) — serial lolos,
  paralel gagal, dispatcher memperbaikinya. Ini mencerminkan batasan produksi
  (domain entri-tunggal).

Isolasi + compat_parity + stabilitas kini PASS pada build linked (arsip soak +
paritas subset + reset per-request mendarat) — `available()` fail-closed hanya
pada build stub.

## Verifikasi gerbang WSL (gerbang M6, 2026-11-19, WSL Debian — historis)

- G1 NTS: php -i menunjukkan Thread Safety disabled, Debug Build no, Zend Extension Build API420230831,NTS. PASS (NTS non-debug).
- G2 toolchain (setelah sudo apt-get install -y php8.3-dev libphp8.3-embed):
  - php-config --version => 8.3.36
  - /usr/include/php/20230831/sapi/embed/php_embed.h ada
  - ldconfig -p | grep libphp => libphp.so => /lib/libphp.so. PASS.
- G3 kontrak: cargo test -p phpx-runtime-embedded => 12 passed, 0 failed di Linux. PASS.
- G4 CLI:
  - phpx runtime list => process aktif (workers=4), embedded experimental-disabled (available=false, linked=false, single-threaded, max_entries=1); php target 8.4 | aktual PHP 8.3.36 NTS.
  - phpx runtime inspect --json => embedded available false, enabled false, single_threaded true, max_concurrent_entries 1, abi sapi embed nts_only true. PASS (stub jujur, tanpa overclaim).

## Opt-in native link (eksperimental, tidak pernah default)

- Build default tetap stub: cargo build -p phpx-runtime-embedded => linked=false, available=false. Box Windows dan ZTS tetap hijau.
- Hanya Linux NTS:
  - sudo apt-get install -y php8.3-dev libphp8.3-embed (php8.4: php8.4-dev libphp8.4-embed)
  - PHPX_EMBED_LINK=1 cargo build -p phpx-runtime-embedded
  - PHPX_EMBED_LINK=1 cargo test -p phpx-runtime-embedded
- build.rs tidak pernah menggagalkan build. Pada toolchain hilang, mismatch versi, atau TARGET non-linux ia mengemis cargo:warning dan tetap stub.
- Bila linked=true, `available` mengikuti asesor gerbang (9/9 PASS + arsip soak). Docker tetap process-runtime-only.
- OTel tetap OTel-ready (passthrough traceparent + JSON log), bukan exporter OTLP. Batas cgroup dan OS tetap tugas operator (Docker --cpus/--memory, systemd MemoryMax/CPUQuota).

## Kompat Opsi-2 eksplisit (2026-11-19, DONE — live sejak serve path P2)

Tidak ada penundaan samar: setiap subset embedded kini menyebut fixture process
hijau-nya DAN status M6-nya (compat.rs Subset.process_fixture + embedded_blocker).

| Subset | Fixture process (hijau hari ini) | Status embedded (gerbang M6) |
|--------|-------------------------------|----------------------------|
| vanilla | examples/vanilla-php (m4_compat: isolasi + crash-recovery hijau) | LIVE via `live::serve` pada build linked (dipin oleh `linked_parity`); stub melaporkan deferred |
| laravel-minimal | examples/laravel-minimal (m4_compat: routing + JSON hijau) | LIVE via `live::serve` pada build linked (routing + JSON + body urlencoded + cookies); stub melaporkan deferred |
| wordpress-minimal | examples/wordpress-minimal (m4_compat: loop + search + shortcode hijau) | LIVE via `live::serve` pada build linked (`?p`/`?s` + cookies); stub melaporkan deferred |
| woocommerce-minimal | examples/woocommerce-minimal (m4_compat: cart + checkout JSON hijau) | LIVE via `live::serve` pada build linked (`?add_to_cart` + checkout JSON); stub melaporkan deferred |

Deferral yang link-aware: run_subset_stub tetap mengandung substring 'deferred' di kedua mode
agar kontrak stabil. Stub melaporkan 'not linked'; subset yang sama tetap hijau
lewat process runtime.

Run gerbang 2026-11-19 (historis):
- Windows (ZTS dev box, stub): cargo test -p phpx-runtime-embedded = 28 lib + 12 kontrak, semua hijau.
- WSL Debian (php-config 8.3.36 NTS, PHPX_EMBED_LINK=1, libphp ter-link): cargo test -p phpx-runtime-embedded = 28 lib + 12 kontrak, semua hijau.
- Kompat process WSL: cargo test -p phpx-server --test m4_compat = 5 passed (vanilla, composer, laravel, wp+woo, metrics).
- runtime inspect (Windows): embedded linked=false, available=false, enabled=false. Stub jujur, tanpa overclaim.

## Serve path gateway (P2, DONE 2026-11-25)

`runtime.mode=embedded` terkabel end-to-end: gateway membangun `EmbeddedPool`
single-flight (`crates/phpx-worker/src/embedded.rs` — antrean terbatas +
timeout eksplisit, kontrak 503/500/504 sama dengan pool process) dan merutekan
rute yang sama (direct .php / statik / front-controller) lewat
`live::serve_with_timeout`. Timeout eksekusi eksplisit: gateway menunggu reply
engine maksimal `request_timeout_seconds` lalu 504 dalam budget; request panjang
tidak bisa di-kill mid-request (batas Beta NTS yang didokumentasikan) dan
antrean menunggu di belakangnya. `runtime inspect`/`list` melaporkan pool
embedded bila mode=embedded. Bukti: `p2_embedded_gateway` 5/5 hijau di stub
(fail-closed) dan linked (live 200 + routing).
