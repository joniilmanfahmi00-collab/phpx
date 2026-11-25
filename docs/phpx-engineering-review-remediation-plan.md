# Rencana Review & Remediasi Engineering PHPX

**Repositori:** `joniilmanfahmi00-collab/phpx`\
**Basis review:** file repositori yang diinspeksi pada 2026-11-20\
**Status:** rekomendasi, bukan klaim implementasi

> Dokumen ini mencatat temuan dan langkah berikut yang direkomendasikan.
> Dokumen ini tidak mengklaim bahwa pekerjaan yang diusulkan sudah
> diimplementasikan atau diuji. Periksa ulang branch terkini sebelum
> bertindak atas tiap butir.

## Status tindak lanjut (2026-11-25)

Sejak review 2026-11-20, seluruh jalur P0-P2 selesai dan terbukti:

- **P0-A**: mode eksekusi eksplisit (`one-shot` vs `persistent`, dilaporkan CLI/JSON).
- **P0-B**: `PersistentPool` terawasi (state machine `Starting→Ready→Busy→Ready`, drain/restart, `Failed` terminal) + test deterministik.
- **P0-C**: guardian RSS OS + `memory_limit` sebagai `-d` PHP + sampling + test soft/hard.
- **P0-D**: Laravel 12 + WordPress 7.1 + WooCommerce 11.3 asli (`p0d_laravel` 5/5, `p0d_wordpress` 13/13).
- **P1-A**: matriks resiliensi 9 test + ambang penerimaan (`resilience.md`).
- **P1-B**: bench lab + repeat + gate CV + multipart `$_POST`/`$_FILES` + driver Redis + autoscale adaptif opt-in.
- **P2**: EmbeddedPool single-flight + `runtime.mode=embedded` terkabel (9/9 gerbang linked, `p2_embedded_gateway` 5/5); CLI melaporkan pool embedded.
- **Soak**: arsip 60 menit (12.600/12.600, +3,6 KB/mnt) + **6 jam (73.600/73.600, gagal 0, -0,86 KB/mnt)** — lihat `docs/operations/soak.md`.

Sisa menuju RC ada di `docs/operations/release-gates.md` (rekor RC di hardware RC, SLO formal, tren 24 jam). Isi review di bawah ini dipertahankan sebagai rekaman historis 2026-11-20.

## 1. Ringkasan eksekutif

PHPX punya arah jelas: platform aplikasi PHP bertenaga Rust yang
dimaksudkan menjalankan aplikasi PHP konvensional tanpa perubahan sumber,
dengan Process Runtime sebagai baseline dan Embedded Runtime eksperimental.

Repositori sudah punya fondasi berguna: workspace Rust modular,
konfigurasi TOML, gateway HTTP, admission eksekusi terbatas, penanganan
timeout dan overload, dokumentasi operasional, packaging Docker, test,
dan desain siklus-hidup worker/manajemen-resource yang diusulkan.

Kesenjangan utama adalah antara model **worker terawasi dan
manajemen-resource** yang dimaksud dengan jalur eksekusi Process Runtime
saat itu. Jalur `phpx-worker` yang diinspeksi memanggil
`phpx_runtime_process::execute_one_shot(...)` untuk tiap request yang
diakui. Ini memberi isolasi process level-request, tetapi belum pool
worker PHP persistent yang terawasi individual dengan health per-worker
dan recycling sadar-RSS.

Langkah berikut seharusnya bukan menambah fitur platform lagi. Melainkan
membuat perilaku lifecycle dan resource Process Runtime eksplisit,
terukur, dan teruji. (Catatan 2026-11-25: sudah dikerjakan via P0-B/P0-C.)

## 2. Framing produk yang harus dijaga

PHPX seharusnya tidak dibingkai sebagai proyek yang tujuan satu-satunya
mengalahkan Go, FrankenPHP, RoadRunner, atau runtime lain pada RPS mentah.

Nilai yang dimaksud lebih luas:

-   Runtime PHP yang dapat diprediksi dan teramati untuk app konvensional.
-   Batas operasional eksplisit (antre, worker, memori, timeout).
-   Kompatibilitas ketat dengan app PHP yang ada.
-   Perkakas operator (CLI, doctor, metrik, panduan deployment).
-   Jalur embedded eksperimental yang jujur tentang statusnya.

## 3. Temuan repositori (snapshot 2026-11-20)

### 3.1 Kekuatan yang ada

Repositori yang diinspeksi menunjukkan: - Workspace Cargo modular dengan
crate terpisah untuk core, config, CLI, backend runtime, worker/server,
dan layanan platform. - Kebijakan Process Runtime dulu. - Embedded
Runtime eksplisit diperlakukan eksperimental dan nonaktif default. -
Spesifikasi teknis yang menjelaskan antre terbatas, backpressure,
envelope request, lifecycle runtime, dan syarat kompatibilitas. -
Usulan ADR-0014 tentang state worker, sinyal health, ambang memori
soft/hard, recycling, proteksi restart, dan scaling adaptif yang
ditunda. - Dokumentasi operasional dan skrip untuk quickstart,
baseline performa, soak test, dan deployment. - Image Docker yang
dirancang jalan sebagai non-root. - Laporan benchmark yang melabeli
hasil mesin developer sebagai non-SLA dan memperingatkan bahwa run
sekuensial Windows dan paralel WSL2 tidak dapat dibandingkan langsung.

### 3.2 Kesenjangan implementasi utama: worker one-shot versus persistent

`crates/phpx-worker/src/lib.rs` yang diinspeksi memakai
`execute_one_shot(...)` untuk tiap request yang diakui. (Catatan 2026-11-25:
kini ada `PersistentPool` P0-B dan `EmbeddedPool` P2 di sampingnya.)

Konsekuensi (saat itu): - Mekanisme admission terbatas saat itu bisa
membatasi eksekusi konkuren dan membatasi request menunggu. - Tiap
request mendapat proses PHP fresh, sehingga state level-process tidak
bertahan antar request dalam worker yang sama. - Desain saat itu belum
menunjukkan pool worker PHP persistent dengan identitas worker
individual dan state lifecycle yang dilacak. - Counter request/recycling
logis tidak boleh disajikan sebagai bukti bahwa proses OS persistent
di-recycle. - RSS per-worker, uptime, jumlah request, alasan keluar,
state health, dan recycling picu-memori butuh identitas worker
persistent dan supervisor.

Ini perbedaan arsitektural, bukan otomatis cacat. Eksekusi one-shot bisa
tetap berguna sebagai mode kompatibilitas atau isolasi. Produk dan
dokumentasi harus membedakannya dari mode worker persistent.
(Status 2026-11-25: dibedakan eksplisit via `execution_mode`.)

### 3.3 Konfigurasi resource belum menjadi guardian penuh yang diusulkan

`RuntimeProcessConfig` yang diinspeksi mencakup jumlah worker, worker
minimum, maksimum request, timeout request/startup, string batas-memori,
queue bound/wait timeout, drain timeout, dan setting restart/backoff.
(Status 2026-11-25: guardian P0-C terimplementasi — sampling RSS per-worker,
recycle graceful soft, terminasi protektif hard.)

Konfigurasi saja tidak membuktikan sampling RSS per-worker level-OS,
recycling graceful ambang-soft, terminasi protektif ambang-hard, atau
kontrol resource adaptif terimplementasi.

Dokumentasikan tiap setting sesuai titik enforcement aktualnya: -
batas level-PHP; - kebijakan supervisi-process PHPX; - enforcement
OS/container; - ambang advisory/diagnostik.

Jangan menyiratkan bahwa `memory_limit` PHP setara batas RSS proses OS.

### 3.4 Status Embedded Runtime

Jaga Embedded Runtime eksperimental dan nonaktif default hingga eksekusi
PHP Embed SAPI nyata, penanganan ABI, batasan lifecycle,
kompatibilitas, dan containment kegagalan tervalidasi. (Catatan 2026-11-25:
syarat ini kini terpenuhi pada build linked — 9/9 gerbang, serve path
terkabel; default tetap `process`.)

Stub kapabilitas atau laporan ketersediaan tidak setara backend eksekusi
PHP embedded yang bekerja.

### 3.5 Bukti kompatibilitas butuh level eksplisit

Fixture minimal berguna untuk test kontrak dan smoke, tetapi tidak setara
test komprehensif terhadap distribusi framework/aplikasi nyata.

Level bukti yang dipakai (lihat `compatibility-matrix.md`):

- Fixture tested — fixture minimal buatan lolos.
- Framework smoke tested — instalasi framework asli boot dan melayani request dasar.
- Integration tested — lifecycle, routing, session, upload, dan dependensi framework yang relevan diuji.
- Application validated — aplikasi nyata representatif lolos suite test terdokumentasi.
- Production validated — deployment dan workload mirip-produksi dilatih dengan batasan terdokumentasi.

Jangan gambarkan Laravel, WordPress, atau WooCommerce sebagai didukung
penuh melampaui bukti terkuat yang tersedia untuk rilis spesifik.
(Status 2026-11-25: level kami = integration tested pada app asli + batas
SAPI eksplisit yang dipin test.)

### 3.6 Bukti benchmark saat ini adalah baseline developer

`docs/operations/performance.md` yang diinspeksi mencatat baseline kecil
mesin developer dan memperingatkan eksplisit bahwa run sekuensial
Windows dan run paralel WSL2 tidak apple-to-apple. Ini disiplin
dokumentasi yang baik.

Hasil ini berguna untuk pelacakan regresi, bukan klaim performa atau
produksi luas. Perluas metodologi sebelum menarik kesimpulan tentang
throughput berkelanjutan, tail latency, efisiensi memori, perilaku
memori jangka-panjang, stabilitas worker persistent, crash recovery,
atau perbandingan dengan alternatif. (Catatan 2026-11-25: lab P1-B +
repeat + arsip soak 60 menit/6 jam memperluas ini; rekor RC tetap
pekerjaan lanjutan.)

## 4. Roadmap implementasi yang direkomendasikan (status: selesai P0-P2)

### P0-A: Buat mode eksekusi eksplisit — DONE

**Tujuan:** cegah eksekusi one-shot tertukar dengan supervisi worker persistent.

Tugas: - Namai dan dokumentasikan jalur eksekusi saat ini sebagai one-shot
bila cocok dengan jalur kode aktual. - Jaga mode one-shot tersedia di mana
trade-off isolasi/kompatibilitasnya berguna. - Definisikan kontrak eksekusi
worker-persistent terpisah. - Definisikan identitas worker dan identitas
inkarnasi. - Definisikan transport request dan versi protokol antara
supervisor Rust dan worker PHP. - Pastikan eksekusi PHP yang blocking tidak
pernah jalan langsung di thread eksekutor Tokio. - Dokumentasikan aplikasi
mana yang bisa memakai mode persistent dan bagaimana batasan reset-state
ditangani.

Kriteria penerimaan: - CLI/konfigurasi melaporkan mode eksekusi aktual yang
dipilih. - Metrik membedakan eksekusi one-shot dari request worker
persistent. - Test membuktikan lifetime process one-shot terlingkup request.
- Mode persistent tidak diiklankan hingga proses PHP berumur-panjang yang
nyata disupervisi.

### P0-B: Implementasikan lifecycle worker persistent deterministik — DONE

**Tujuan:** bangun pool worker terawasi berukuran-tetap sebelum scaling adaptif.

State yang disarankan (terimplementasi):

`Starting -> Ready -> Busy -> Ready`

Transisi tambahan: - `Ready -> Draining -> Stopped` - `Busy -> Draining -> Stopped`
setelah request berjalan - `Busy -> Unhealthy -> Restarting -> Ready` - `Starting -> Failed`

Tugas: - Spawn sejumlah worker PHP terkonfigurasi berukuran-tetap. - Lacak
PID, ID worker, inkarnasi, state, waktu start, jumlah request selesai, dan
alasan keluar. - Implementasikan handshake startup dan timeout readiness. -
Jangan tugaskan lebih dari satu request per worker process secara default.
- Implementasikan antre terbatas dan perilaku overload eksplisit. - Drain
worker saat reload dan shutdown. - Terapkan backoff restart, jitter, budget
restart rolling, dan proteksi crash-loop. - Hindari mengganti semua worker
serentak saat recycling rutin.

Kriteria penerimaan: - Transisi state worker diliput test deterministik. -
Crash worker tidak menjatuhkan gateway. - Pengganti menjadi ready hanya
setelah handshake sukses. - Kapasitas antre dan waktu tunggu terbatas dan
teramati. - Shutdown graceful men-drain kerja in-flight dalam deadline
terkonfigurasi. - Kegagalan startup berulang berhenti pada budget restart
terkonfigurasi dan mengekspos alasan diagnostik.

### P0-C: Implementasikan Resource Health & Memory Guardian — DONE

**Tujuan:** jadikan manajemen resource loop kontrol yang teramati, bukan
sekadar janji konfigurasi.

Kumpulkan per worker bila didukung: - RSS dan peak RSS; - uptime; - jumlah
request selesai; - durasi request; - durasi startup dan shutdown; - exit code
dan alasan terminasi; - jumlah dan frekuensi restart; - state worker saat
ini; - cek health sukses terakhir.

Kebijakan terpisah:

**Ambang memori soft** - Tandai worker untuk recycling. - Hentikan
pemberian kerja baru. - Biarkan request berjalan selesai dalam deadline
drain. - Matikan dan ganti worker.

**Ambang memori hard** - Picu aksi protektif terdokumentasi. - Bila
recovery graceful tak lagi aman, terminasi worker. - Catat alasan dan hasil
recovery.

Batasan penting: - Interval sampling dan latensi terminasi bisa menyebabkan
overshoot ambang. - RSS tidak sama dengan pemakaian heap PHP. - RSS stabil
tidak membuktikan tidak ada bocor. - RSS tetap tinggi tidak otomatis
membuktikan bocor. - Perilaku alokator PHP, ekstensi nativ, shared library,
dan fragmentasi memori memengaruhi memori teramati.

Kriteria penerimaan: - Metrik mengekspos memori dan data lifecycle
per-worker. - Recycling ambang-soft diuji dengan beban memori terkendali. -
Aksi ambang-hard diuji di lingkungan terisolasi. - Overshoot ambang dan
interval sampling didokumentasikan. - Penggantian dan recovery worker
teramati. - Tidak ada klaim perilaku bebas-bocor hanya dari pengukuran RSS.

### P0-D: Bangun test kompatibilitas nyata — DONE

**Tujuan:** validasi kompatibilitas ketat memakai aplikasi PHP aktual.

Mulai dengan: 1. Aplikasi PHP vanilla. 2. Aplikasi berbasis Composer yang
nyata. 3. Instalasi Laravel nyata. 4. Instalasi WordPress nyata.
5. WooCommerce setelah cakupan WordPress andal.

Uji minimal: - routing front-controller; - `$_SERVER`, method request,
query string, dan header; - body request dan JSON; - cookie dan session; -
upload file; - header dan status respons; - environment variable; - autoload
Composer; - penanganan error dan buffering output; - aset statik dan
keamanan path; - perilaku shutdown; - bocor state worker-persistent, bila
mode persistent aktif.

Publikasikan matriks versi/ekstensi per rilis. Tandai ekstensi tak didukung
dan batasan runtime eksplisit.

Kriteria penerimaan: - Test berjalan terhadap paket framework/aplikasi
upstream nyata. - Tiap klaim kompatibilitas tertaut ke bukti test relevan.
- Fixture dan test aplikasi nyata dilaporkan terpisah. - Tidak perlu
modifikasi sumber untuk baseline kompatibilitas konvensional, kecuali
batasannya didokumentasikan jelas.

### P1-A: Test resiliensi dan soak — DONE

Tambah test berulang untuk: - crash worker saat idle; - crash worker saat
memproses request; - timeout request; - gagal startup; - proteksi
restart-loop; - reload graceful; - shutdown graceful dengan request
in-flight; - saturasi antre dan respons overload; - recycling ambang-memori;
- tren memori jangka-panjang; - bocor state antar request.

Jalankan soak test pendek dan panjang. Arsipkan log mentah, metadata
lingkungan, konfigurasi, dan ringkasan hasil. (Arsip: 60 menit + 6 jam.)

Ukur waktu recovery dari event yang didefinisikan jelas: - timestamp
deteksi kegagalan; - timestamp restart diinisiasi; - timestamp pengganti
ready; - timestamp request sukses pertama.

Laporkan terpisah, jangan pakai satu "waktu recovery" yang ambigu.

### P1-B: Laboratorium benchmark — DONE

Jaga performa, efisiensi resource, stabilitas, dan reliabilitas sebagai
dimensi terpisah (throughput, latensi p50/p95/p99/maks, cold start, jalur
warm, efisiensi resource, stabilitas memori tren 1j/6j/24j, reliabilitas,
operasi). Aturan banding: hardware dan OS sama, versi PHP dan ekstensi sama,
app dan datastore sama, mix dan payload request sama, konkurensi dan durasi
sama, tuning dan batas worker setara, build release untuk klaim performa,
laporkan pengukuran dan konfigurasi mentah. (Lab + repeat + gate CV +
multipart + Redis selesai.)

Bandingkan PHPX dengan alternatif relevan hanya bila setup adil dan dapat
diulang. Jangan mengarang target RPS absolut sebelum workload representatif
mapan.

### P2: Manajemen worker adaptif — DONE (opt-in)

Tunda scaling dinamis hingga perilaku pool-tetap stabil. (Selesai: sinyal
kedalaman antre + jumlah aktif vs kapasitas terspawn; pengaman min/maks,
hysteresis, cooldown, memory guard, tidak pernah saat drain; nonaktif
default hingga bukti berulang.)

### P2: Embedded Runtime — DONE (serve path terkabel)

Jaga Embedded Runtime eksperimental dan nonaktif default. (Terpenuhi.)

Gerbang promosi (semua lolos pada build linked): - eksekusi PHP Embed SAPI
nyata; - build PHP dan matriks ABI yang didukung; - domain eksekusi
terkendali dan batasan thread; - aturan ownership/lifetime jelas lintas C
ABI; - test kompatibilitas ekstensi; - test isolasi state request; - strategi
containment kegagalan; - paritas pada suite kompatibilitas untuk tiap
kapabilitas yang diklaim.

Jangan promosikan berdasar throughput semata.

## 5. Metrik dan terminologi yang disarankan

Pakai nama presisi dan hindari counter agregat ambigu.

### Metrik worker

-   `phpx_worker_state`
-   `phpx_worker_rss_bytes`
-   `phpx_worker_peak_rss_bytes`
-   `phpx_worker_uptime_seconds`
-   `phpx_worker_requests_total`
-   `phpx_worker_restarts_total{reason=...}`
-   `phpx_worker_startup_duration_seconds`
-   `phpx_worker_shutdown_duration_seconds`

(Catatan 2026-11-25: seri `phpx_pool_*` dan `phpx_worker_*` terimplementasi;
lihat `metrics.md`.)

### Metrik antre dan gateway

-   `phpx_requests_total{outcome=...}`
-   `phpx_request_duration_seconds`
-   `phpx_queue_depth`
-   `phpx_queue_wait_duration_seconds`
-   `phpx_overload_rejections_total`
-   `phpx_worker_capacity`
-   `phpx_worker_active`

Nama metrik adalah usulan. Selaraskan dengan konvensi penamaan repositori
sebelum implementasi. Hindari label kardinalitas-tinggi seperti ID request,
URL mentah, atau pengenal pengguna.

### Pengukuran recovery

Catat: - latensi deteksi; - jeda penjadwalan restart; - durasi startup
worker; - jeda readiness; - waktu ke request sukses pertama; - durasi
recovery total. (Terimplementasi via `WorkerSnapshot::last_recovery` +
seri `phpx_worker_last_recovery_duration_seconds`.)

## 6. Koreksi dokumentasi

Perbarui dokumentasi agar perilaku terimplementasi, perilaku teruji,
perilaku eksperimental, dan perilaku terencana terlihat berbeda.

Label kapabilitas yang direkomendasikan: - **Implemented:** jalur kode ada. -
**Tested:** bukti test otomatis atau terdokumentasi ada. -
**Experimental:** tersedia untuk evaluasi, bukan kontrak stabil. -
**Stub:** antarmuka atau placeholder ada, tetapi eksekusi nyata tidak ada. -
**Deferred:** sengaja ditunda. - **Unsupported:** eksplisit tidak didukung.

Pekerjaan dokumentasi spesifik (status 2026-11-25): - README: meringkas
kapabilitas dan batasan v0.1 aktual — DONE (diterjemahkan Indonesia). -
Spesifikasi teknis: membedakan arsitektur target dari implementasi kini —
DIPERBARUI (lihat TECHNICAL_SPEC_v0.1.md). - ADR-0014: pertahankan sebagai
proposed hingga diterima; tautkan test implementasi — SEBAGIAN (test
tertulis; status formal tetap Proposed). - Laporan performa: pertahankan
baseline developer kini dan tambah hasil release-mode/soak terpisah —
DONE (lab + arsip soak 60 mnt/6 jam). - Matriks kompatibilitas: pisahkan
fixture dari validasi framework/aplikasi nyata — DONE. - CLI help:
laporkan kapabilitas runtime aktual, bukan aspirasional — DONE (inspect
melaporkan pool embedded bila mode=embedded).

## 7. Gerbang rilis

Jangan promosikan rilis hanya karena daftar fitur rencana selesai.

### Developer preview — DONE

-   Gateway dasar dan jalur eksekusi bekerja.
-   Keterbatasan yang diketahui didokumentasikan.
-   Tanpa SLA produksi tersirat.

### Alpha — DONE

-   Siklus hidup worker tetap deterministik.
-   Antre terbatas dan backpressure teruji.
-   Perilaku crash dan restart worker teramati.
-   Test kompatibilitas inti lolos.

### Beta — DONE

-   Resource guardian diimplementasikan dan teruji.
-   Smoke/integration test Laravel dan WordPress nyata ada.
-   Soak dan failure-injection dapat diulang (arsip 60 menit + 6 jam).
-   Keterbatasan keamanan dan deployment didokumentasikan.
-   Serve path embedded terkabel dengan fail-closed jujur.

### Release candidate — NOT DONE

-   Benchmark mode-rilis dapat diulang. (Parsial: metodologi + arsip soak 6 jam ada; rekor RC hardware RC belum.)
-   Perilaku recovery dan memori memenuhi kriteria penerimaan terdokumentasi. (Ambangan ada; SLO formal belum.)
-   Matriks kompatibilitas dipublikasikan. (DONE.)
-   Tanpa isu keamanan atau integritas-data kritis yang belum selesai. (DONE.)

### Stabil 1.0 — NOT DONE

-   Semua klaim 1.0 yang dipublikasikan punya bukti test.
-   Prosedur upgrade, rollback, operasional didokumentasikan.
-   Matriks PHP/OS/ekstensi yang didukung eksplisit.
-   Artefak rilis dapat direproduksi dan terverifikasi.

## 8. Tindakan berikut segera (urutan yang direkomendasikan — semua selesai)

1.  ~~Audit dan labeli jalur eksekusi one-shot kini secara akurat.~~ DONE (P0-A).
2.  ~~Tulis protokol dan kontrak lifecycle worker persistent.~~ DONE (P0-B + `persistent-worker-contract.md`).
3.  ~~Implementasikan supervisi worker persistent berukuran-tetap.~~ DONE (P0-B).
4.  ~~Tambah identitas worker, transisi state, dan metrik lifecycle.~~ DONE (P0-B).
5.  ~~Implementasikan sampling RSS per-worker dan kebijakan memori soft/hard.~~ DONE (P0-C).
6.  ~~Tambah test crash, drain, budget-restart, dan ambang-memori deterministik.~~ DONE (P1-A).
7.  ~~Tambah smoke test Laravel dan WordPress nyata.~~ DONE (P0-D).
8.  ~~Perluas bukti benchmark dan soak mode-rilis.~~ DONE (lab P1-B + arsip soak 60 mnt/6 jam).
9.  ~~Kaji ulang scaling adaptif hanya setelah baseline pool-tetap stabil.~~ DONE (P2 opt-in).

## 9. Kesimpulan reviewer

PHPX punya arah produk yang koheren dan groundwork arsitektural yang
berguna. Risiko engineering terpenting bukan pilihan Rust atau PHP;
melainkan apakah runtime benar-benar mengimplementasikan dan membuktikan
perilaku lifecycle worker, monitoring resource, recycling, kompatibilitas,
dan recovery yang ingin ditawarkan produk.

Milestone berikut harus memprioritaskan **perilaku runtime terukur atas
jumlah fitur**. (Catatan 2026-11-25: prioritas ini dipenuhi — P0-A…P2
selesai dengan bukti; fokus kini: rekor RC, SLO formal, tren 24 jam.)

> Bangun control loop. Ukur resource. Injeksi kegagalan. Buktikan
> recovery. Baru buat klaim.
