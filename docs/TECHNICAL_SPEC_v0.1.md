# Spesifikasi Teknis PHPX v0.1

**Status:** Spesifikasi baseline\
**Versi:** 0.1.0\
**Tanggal:** 2026-11-17 (diterjemahkan ke Indonesia + pembaruan status 2026-11-25)\
**Produk:** PHPX

## 1. Ringkasan

PHPX adalah platform aplikasi PHP bertenaga Rust. Ia menyediakan gateway
HTTP, manajemen runtime PHP, worker terawasi, dan jalur menuju antre,
penjadwalan, cache, storage, dan observabilitas yang terintegrasi.

PHPX tidak menggantikan PHP sebagai bahasa aplikasi. Ia menjalankan
aplikasi PHP yang ada dengan target kompatibilitas ketat dan menyediakan
kapabilitas platform opsional.

### 1.1 Tujuan produk

1.  Menjalankan aplikasi PHP konvensional tanpa perubahan kode sumber.
2.  Menyediakan satu antarmuka operasional untuk lifecycle server dan worker PHP.
3.  Mendukung deployment Linux nativ dan Docker.
4.  Mendukung backend runtime PHP process dan embedded di balik abstraksi bersama.
5.  Menyediakan default aman, pemakaian resource terbatas, dan diagnostik actionable.
6.  Tumbuh dari inti eksekusi HTTP/PHP yang andal menjadi platform aplikasi lengkap.

### 1.2 Non-tujuan v0.1

-   Menulis ulang bahasa PHP atau Zend Engine.
-   Menggantikan Composer atau ekstensi PHP.
-   Mengklaim kompatibilitas universal dengan setiap SAPI atau ekstensi PHP.
-   Dukungan produksi Windows nativ.
-   HTTP/3.
-   Isolasi kode-bermusuhan multi-tenant yang dijamin.
-   Menggantikan Redis, PostgreSQL, atau object storage.
-   Menjadikan embedded runtime backend produksi default.

## 2. Baseline platform yang didukung

### 2.1 Sistem operasi

-   Linux adalah host v0.1 primer dan yang didukung.
-   Deployment Docker menarget kontainer Linux.
-   Pengembangan Windows boleh memakai WSL2 atau Docker Desktop.
-   macOS boleh dipakai untuk pengembangan di mana dependensi memungkinkan,
    tetapi bukan komitmen dukungan produksi v0.1.

### 2.2 Versi PHP

Target kompatibilitas awal adalah PHP 8.3, 8.4, dan 8.5, tunduk pada
ketersediaan build upstream yang didukung, kompatibilitas ABI, dan cakupan
CI. Matriks yang didukung harus dipublikasikan per rilis.

Process Runtime adalah jalur kompatibilitas baseline. Dukungan Embedded
Runtime awalnya boleh mencakup matriks yang lebih sempit.

### 2.3 Rust

Pakai toolchain Rust stabil yang di-pin di `rust-toolchain.toml`. Minimum
supported Rust version (MSRV) ditetapkan setelah pemilihan dependensi dan
validasi CI, lalu dicatat di metadata rilis.

## 3. Arsitektur sistem

### 3.1 Komponen logis

-   **CLI:** parsing perintah, diagnostik, operasi runtime dan platform.
-   **Core:** tipe bersama, identitas aplikasi, error, kontrak lifecycle.
-   **Config:** pemuatan TOML, override environment, validasi, default.
-   **Gateway:** listener, TLS, penanganan protokol HTTP, aset statik.
-   **Router:** dispatch host/path dan pemetaan front-controller PHP.
-   **Runtime Manager:** memilih dan menyupervisi backend runtime.
-   **Process Runtime:** men-start dan berkomunikasi dengan proses anak PHP.
-   **Embedded Runtime:** bridge PHP Embed SAPI eksperimental (live pada build linked sejak P2).
-   **Worker Manager:** ukuran pool, readiness, health, recycling, draining.
-   **Queue:** lifecycle job dan adapter backend.
-   **Scheduler:** dispatch tugas berulang dan tertunda.
-   **Cache:** cache lokal dan adapter cache eksternal.
-   **Storage:** adapter object storage lokal dan kompatibel-S3.
-   **Observabilitas:** log, metrik, trace, endpoint health.
-   **Keamanan:** perizinan, batas, penanganan secret, kebijakan isolasi.

### 3.2 Siklus hidup request

1.  Terima koneksi di gateway Rust.
2.  Parse dan validasi request HTTP.
3.  Selesaikan aplikasi dan rute.
4.  Sajikan aset statik yang memenuhi syarat langsung.
5.  Untuk request dinamis, bangun envelope eksekusi terbatas.
6.  Akuisisi worker PHP yang tersedia untuk runtime terkonfigurasi.
7.  Teruskan metadata dan body request lewat transport terpilih.
8.  Eksekusi entry point aplikasi.
9.  Kembalikan status, header, dan body respons ke gateway.
10. Lepas atau recycle worker sesuai health dan kebijakan.
11. Emisikan telemetri request dan runtime yang terkorelasi.

Gateway harus menerapkan backpressure bila kapasitas worker tak tersedia.
Gateway tidak boleh membuat antre request tertunda tak terbatas.

### 3.3 Envelope request

Batas runtime harus mendefinisikan envelope request berversi yang minimal berisi:

-   versi protokol;
-   ID request dan konteks trace;
-   method;
-   scheme;
-   host;
-   path dan query string;
-   header ternormalisasi;
-   metadata alamat remote bila tersedia;
-   panjang konten dan stream body atau payload body terbatas;
-   root aplikasi dan entry point;
-   metadata deadline/timeout;
-   pengenal profil environment.

Envelope respons berisi:

-   ID request;
-   kode status;
-   header respons;
-   stream body respons atau payload terbatas;
-   durasi eksekusi;
-   outcome worker dan klasifikasi diagnostik.

Normalisasi header harus menjaga semantik yang terlihat aplikasi dan tidak
boleh memungkinkan penyalahgunaan header hop-by-hop. Penanganan body harus
terbatas dan mendukung streaming di mana implementasi transport memungkinkan.

## 4. Backend runtime

### 4.1 Kontrak runtime bersama

Abstraksi runtime harus mengekspos operasi lifecycle yang secara konseptual
setara:

-   `start(config)`
-   `health()`
-   `execute(request)`
-   `drain()`
-   `recycle(reason)`
-   `shutdown()`
-   `capabilities()`

API Rust aktual boleh memakai method async untuk orkestrasi, tetapi eksekusi
engine yang blocking tidak boleh jalan di thread eksekutor Tokio.

### 4.2 Process Runtime

**Status:** Wajib untuk baseline v0.1.

Tanggung jawab:

-   Temukan dan validasi executable PHP.
-   Verifikasi versi PHP dan ekstensi yang dibutuhkan.
-   Start worker anak PHP.
-   Bangun IPC terbatas.
-   Lacak ID proses dan status keluar.
-   Tegakkan deadline request dan batas resource process bila tersedia.
-   Restart worker tak sehat dengan backoff.
-   Drain worker saat reload dan shutdown.

Pemilihan transport:

-   Prototipe protokol Unix Domain Socket berbingkai untuk worker PHP CLI.
-   Evaluasi FastCGI sebagai mode interoperabilitas.
-   Jangan asumsikan PHP-FPM dibutuhkan.
-   Jaga implementasi transport di balik antarmuka internal.

Bootstrap worker PHP harus kecil, auditable, dan kompatibel dengan semantik
request PHP normal. Worker berumur-panjang harus me-reset state terlingkup
request atau di-recycle. Bila reset andal tidak bisa dijamin untuk sebuah
aplikasi, pakai worker satu-request atau mode eksekusi konvensional yang
kompatibel.

### 4.3 Embedded Runtime

**Status:** Eksperimental; nonaktif default. Live pada build linked sejak
P2 (9/9 gerbang promosi, serve path terkabel, `runtime.mode=embedded`).

Pertimbangan implementasi:

-   Pakai PHP Embed SAPI dan bridge C ABI yang sempit.
-   Pin dan validasi konfigurasi build dan ABI PHP.
-   Jaga panggilan lifecycle Zend/PHP dalam domain eksekusi terkendali.
-   Jangan panggil PHP NTS konkuren dari thread sembarang.
-   Hindari menahan lock Rust lintas callback ke PHP.
-   Definisikan aturan ownership dan lifetime untuk string, buffer, dan
    resource engine.
-   Bendung crash memakai batas process bila diperlukan.
-   Iklankan kapabilitas dan batasan saat runtime.

Embedded Runtime harus lolos suite kompatibilitas aplikasi yang sama dengan
Process Runtime untuk setiap kapabilitas yang diklaimnya. Ia tidak boleh
dipromosikan ke default produksi hanya berdasar test throughput.

## 5. Manajemen worker

### 5.1 State worker

`Starting -> Ready -> Busy -> Ready`

Transisi tambahan:

-   `Ready -> Draining -> Stopped`
-   `Busy -> Draining -> Stopped` setelah request berjalan
-   `Busy -> Unhealthy -> Restarting -> Ready`
-   `Starting -> Failed` (Failed terminal untuk inkarnasi worker itu, dengan diagnostik terlihat operator; Stopped adalah stop graceful/terdrain. Lifecycle penuh, budget restart, dan kebijakan memori soft/hard disempurnakan di ADR-0014 yang memperluas seksi ini.)

### 5.2 Kebijakan worker

Kontrol yang dapat dikonfigurasi:

-   worker minimum dan maksimum;
-   request konkuren maksimum per worker (default 1 untuk worker process);
-   request maksimum sebelum recycling;
-   timeout eksekusi request;
-   timeout startup;
-   timeout drain graceful;
-   backoff restart;
-   interval health-check;
-   target memori per-worker (disempurnakan sebagai batas soft untuk recycle graceful dan batas hard untuk terminasi protektif di ADR-0014; overshoot mungkin karena latensi sampling/terminasi);
-   timeout tunggu antre.

Batas pool worker harus terbatas. Crash worker tidak boleh menjatuhkan gateway.

### 5.3 Backpressure

Bila semua worker sibuk:

-   antrekan hanya hingga bound terkonfigurasi;
-   tolak atau timeout-kan kelebihan request secara dapat diprediksi;
-   ekspos metrik saturasi;
-   hindari pertumbuhan memori tak terbatas.

## 6. Gateway HTTP

### 6.1 Syarat v0.1

-   Dukungan HTTP/1.1.
-   Target HTTP/2, digerbangi integration test.
-   TLS via Rustls.
-   Routing host dan path.
-   Dispatch front-controller PHP.
-   Serving file statik.
-   Batas request dan respons yang dapat dikonfigurasi.
-   Shutdown graceful.
-   ID request dan access log terstruktur.

### 6.2 Ditunda

-   HTTP/3.
-   Perilaku CDN bawaan.
-   Paritas fitur reverse-proxy penuh dengan Nginx/Caddy.
-   Fungsionalitas WAF lanjutan.

### 6.3 Keamanan file statik

-   Selesaikan path relatif ke document root terkonfigurasi.
-   Tolak traversal path.
-   Jangan ikuti symlink di luar root yang diizinkan kecuali dikonfigurasi eksplisit.
-   Set tipe MIME dengan aman.
-   Dukung request kondisional dan range saat implementasi matang.
-   Jangan pernah mengekspos file sumber PHP sebagai konten statik.

## 7. Layanan platform

### 7.1 Queue

API antre harus mendukung operasi enqueue, reserve/claim, acknowledge, gagal,
retry, tunda, dan dead-letter.

Driver awal:

-   in-memory: hanya pengembangan/testing, non-durable;
-   SQLite: baseline tahan-lama satu-node;
-   Redis: adapter opsional.

Semantik yang disyaratkan:

-   acknowledgement tahan-lama untuk driver tahan-lama;
-   retry terbatas;
-   backoff eksponensial dengan jitter;
-   visibility timeout atau recovery lease;
-   panduan idempotensi;
-   penanganan dead-letter;
-   shutdown graceful worker.

Eksekusi tepat-sekali tidak dijanjikan. Konsumen harus dirancang untuk
delivery at-least-once dan penanganan job idempoten.

### 7.2 Scheduler

-   Ekspresi kompatibel-cron.
-   Jadwal sadar-timezone.
-   Rekam eksekusi persisten bila dikonfigurasi.
-   Proteksi dispatch-duplikat untuk baseline satu-node.
-   Kebijakan run-terlewat harus eksplisit.
-   Scheduler mendispatch job; ia tidak mengeksekusi kerja PHP berumur-panjang
    di thread gateway.

### 7.3 Cache

Implementasi awal:

-   cache in-memory terbatas;
-   cache filesystem;
-   adapter Redis opsional.

OPcache PHP tetap milik runtime PHP. Cache PHPX bukan pengganti OPcache.

### 7.4 Storage dan session

Abstraksi storage menarget filesystem lokal dulu dan storage
kompatibel-S3 kemudian. Penanganan session harus mendukung perilaku
session PHP konvensional bila memungkinkan. API session nativ-platform
mana pun bersifat opsional.

## 8. Konfigurasi

File primer: `phpx.toml`.

Contoh ilustratif konfigurasi:

``` toml
schema_version = 1

[server]
host = "127.0.0.1"
port = 8080
tls = false
request_body_limit = "16MB"

[application]
root = "."
document_root = "public"
entrypoint = "index.php"

[runtime]
mode = "process"
php_version = "8.4"

[runtime.process]
workers = 4
max_requests = 1000
request_timeout_seconds = 30
startup_timeout_seconds = 10
memory_limit = "256M" # disederhanakan; model penuh memakai [runtime.process.lifecycle] + [runtime.process.memory] batas soft/hard per ADR-0014

[runtime.embedded]
enabled = false

[queue]
driver = "sqlite"
database = "./data/queue.db"
workers = 2

[scheduler]
enabled = false

[cache]
driver = "memory"
max_memory = "128MB"

[observability]
json_logs = true
metrics = true
```

Ini ilustratif dan disederhanakan, bukan skema beku. Model
lifecycle/memori/adaptif detail (\[runtime.process.lifecycle\],
\[runtime.process.memory\], \[runtime.process.adaptive\] dengan adaptif
nonaktif default) didefinisikan di ADR-0014 Seksi 2.9 dan didahulukan
untuk manajemen resource worker. Semua field butuh validasi bertipe,
default terdokumentasi, dan aturan migrasi/versionalisasi. Secret tidak
boleh disimpan di konfigurasi yang di-commit.

## 9. Kontrak CLI

Keluarga perintah awal:

``` text
phpx serve
phpx serve --config phpx.toml
phpx doctor
phpx version
phpx runtime list
phpx runtime inspect
phpx workers status
phpx workers reload
phpx queue status
phpx queue work
phpx scheduler run
phpx metrics
```

Output CLI harus human-readable secara default dan menawarkan JSON
machine-readable bila praktis. Exit code harus stabil dan
terdokumentasi.

## 10. Syarat keamanan

-   Jalan sebagai user non-root secara default bila memungkinkan.
-   Pisahkan tanggung jawab supervisor dari eksekusi aplikasi.
-   Batasi ukuran request, ukuran antre, jumlah worker, dan waktu
    eksekusi.
-   Jangan ekspos secret di log atau diagnostik.
-   Batasi akses filesystem aplikasi ke root terkonfigurasi bila
    praktis.
-   Validasi path file statik terhadap traversal dan symlink escape.
-   Terapkan batas memori dan CPU OS/container untuk workload produksi.
-   Perlakukan ekstensi PHP dan kode aplikasi sebagai kode executable
    tepercaya kecuali sandbox eksplisit dikonfigurasi.
-   Jangan klaim isolasi multi-tenant kuat di v0.1.

## 11. Observabilitas

Baseline yang disyaratkan:

-   log terstruktur;
-   propagasi ID request;
-   event lifecycle worker;
-   health readiness dan liveness;
-   jumlah dan latensi request;
-   jumlah worker aktif, idle, sibuk, gagal;
-   durasi eksekusi PHP;
-   kedalaman antre dan outcome job;
-   counter restart dan timeout.

Metrik Prometheus adalah antarmuka metrik awal. Tracing OpenTelemetry
diperkenalkan bertahap. Header sensitif, token, cookie, dan body request
harus disunting secara default.

## 12. Testing dan kompatibilitas

### 12.1 Lapisan test

-   unit test untuk tipe inti, config, routing, dan state machine;
-   integration test process memakai banyak versi PHP;
-   integration test HTTP;
-   contract test runtime bersama kedua backend;
-   fixture aplikasi untuk PHP vanilla, Laravel, dan WordPress;
-   test durabilitas dan recovery queue;
-   test failure injection dan crash worker;
-   test beban dan soak;
-   test keamanan untuk traversal path, batas request, dan batas
    privilege.

### 12.2 Fixture kompatibilitas

Fixture awal minimum:

1.  Front controller PHP vanilla.
2.  Autoloading Composer.
3.  Lifecycle request Laravel dan middleware umum.
4.  Front controller WordPress, permalink, upload, cookie, dan admin.
5.  Smoke test checkout/session WooCommerce bila baseline WordPress
    stabil.

Test harus memverifikasi status respons, header, cookie, body request,
parameter query, perilaku filesystem, dan penanganan error.

### 12.3 Gerbang rilis

Kandidat rilis mensyaratkan:

-   tanpa isu keamanan kritis yang diketahui;
-   startup dan shutdown deterministik;
-   recovery crash worker;
-   perilaku terbatas di bawah saturasi;
-   tanpa kontaminasi silang request di test worker persistent;
-   hasil kompatibilitas dipublikasikan untuk tiap kombinasi
    runtime/backend yang diklaim;
-   batasan terdokumentasi.

## 13. Metodologi performa

Jangan publikasikan klaim performa tanpa test yang dapat direproduksi.

Benchmark terhadap baseline yang sesuai termasuk Nginx + PHP-FPM dan, bila
relevan untuk klaim lifecycle worker, FrankenPHP, RoadRunner, dan
Swoole/OpenSwoole, memakai build PHP, ekstensi, kode aplikasi, hardware,
dan workload identik.

Ukur:

-   throughput;
-   latensi p50, p95, dan p99;
-   memori CPU dan RSS;
-   waktu startup;
-   perilaku restart worker;
-   saturasi dan recovery;
-   beban berkelanjutan dan pertumbuhan memori.

Laporkan hasil endpoint PHP sederhana maupun workload Laravel/WordPress
yang representatif. Microbenchmark yang lebih cepat bukan bukti cukup
untuk kesiapan produksi.

## 14. Tata letak crate awal

Mulai dengan workspace kecil dan belah crate saat batas menjadi
konkret:

``` text
phpx/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── phpx.toml.example
├── crates/
│   ├── phpx-cli/
│   ├── phpx-core/
│   ├── phpx-config/
│   ├── phpx-runtime/
│   ├── phpx-runtime-process/
│   ├── phpx-runtime-embedded/
│   ├── phpx-worker/
│   ├── phpx-server/
│   ├── phpx-queue/
│   ├── phpx-scheduler/
│   ├── phpx-cache/
│   └── phpx-observability/
├── integrations/
│   ├── laravel/
│   └── wordpress/
├── examples/
│   └── vanilla-php/
├── tests/
│   ├── integration/
│   ├── compatibility/
│   ├── stress/
│   └── security/
└── docs/
    ├── adr/
    ├── architecture/
    └── operations/
```

Jangan buat crate kosong hanya agar cocok dengan tree ini. Perkenalkan tiap
crate bila ada kode dan tanggung jawab stabil.

Catatan pemetaan: phpx-server mengimplementasikan Gateway di Seksi 3.1; phpx-worker mengimplementasikan supervisi Worker Manager; phpx-runtime memegang kontrak runtime bersama. Storage, kebijakan Security, dan Router mulai sebagai modul di dalam crate eksisting dan belah menjadi crate khusus hanya bila batas stabil. Dok tinggal di docs/: adr/, ARCHITECTURE_DECISIONS.md, TECHNICAL_SPEC_v0.1.md, architecture/overview.md, architecture/m1-notes.md, dan operations/linux-prerequisites.md. Router, Storage, dan kebijakan Security mulai sebagai modul di dalam crate eksisting dan belah hanya bila batas stabil.

## 15. Kriteria penerimaan v0.1

Developer preview PHPX v0.1 dapat diterima bila:

1.  `phpx doctor` mendeteksi PHP dan melaporkan prasyarat yang hilang.
2.  `phpx serve` men-start gateway Rust.
3.  Aplikasi PHP vanilla dilayani lewat Process Runtime.
4.  Aset statik disajikan aman.
5.  Banyak worker PHP disupervisi.
6.  Crash worker terdeteksi dan pulih tanpa menjatuhkan gateway.
7.  Konkurensi dan antrean request dibatasi.
8.  Shutdown graceful men-drain request aktif dalam batas terkonfigurasi.
9.  Quickstart Docker dan Linux nativ bekerja.
10. Error konfigurasi actionable.
11. Log dasar, health check, dan metrik tersedia.
12. Keterbatasan kompatibilitas yang diketahui didokumentasikan.

Queue, scheduler, embedded runtime, dan kompatibilitas framework luas
telah dikirim bertahap (M5-M6, P2); jalur eksekusi HTTP/PHP pertama yang
andal tidak tertunda olehnya.
