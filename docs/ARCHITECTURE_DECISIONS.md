# Catatan Keputusan Arsitektur PHPX

**Proyek:** PHPX\
**Versi dokumen:** 0.1.0\
**Status:** Baseline Accepted, tunduk pada validasi implementasi\
**Terakhir diperbarui:** 2026-11-25 (jalur serve embedded P2 terkabel; soak 6 jam terarsip; ADR-0014 tetap Proposed, belum Accepted)

Direktori ini mencatat keputusan arsitektural yang membentuk PHPX secara
material. ADR bersifat imutabel setelah diterima: mengganti keputusan
membutuhkan ADR baru yang merujuk ADR sebelumnya.

## ADR-0001: Identitas dan lingkup produk

-   **Status:** Accepted
-   **Keputusan:** Produk bernama **PHPX** dan merupakan platform aplikasi
    PHP bertenaga Rust, bukan sekadar server HTTP atau manajer proses PHP.
-   **Konteks:** Lingkup khusus-server akan membiarkan pemrosesan antre,
    penjadwalan, siklus hidup runtime, integrasi storage, dan perkakas
    operasional terfragmentasi.
-   **Konsekuensi:**
    -   PHPX menyediakan CLI dan model konfigurasi yang menyatu dan
        menghadap developer.
    -   Platform tersusun dari subsistem yang dapat diuji independen.
    -   Rilis pertama memprioritaskan jalur eksekusi HTTP + PHP yang andal;
        layanan lain ditambah bertahap.
    -   Nama PHPX adalah identitas produk kerja dan harus melalui cek
        repositori, paket, domain, dan merek dagang sebelum rilis komersial.

## ADR-0002: Rust adalah bidang kendali platform

-   **Status:** Accepted
-   **Keputusan:** Rust memiliki infrastruktur platform: gateway, manajemen
    siklus hidup, supervisi worker, penjadwalan, koordinasi antre,
    kontrol resource, observabilitas, dan CLI.
-   **Konteks:** Rust menawarkan kontrol resource yang dapat diprediksi dan
    kapabilitas pemrograman sistem yang kuat. PHP tetap bahasa aplikasi dan
    mesin eksekusi.
-   **Konsekuensi:**
    -   Kode aplikasi PHP tidak ditulis ulang ke Rust.
    -   Batas platform-ke-runtime eksplisit dan berversi.
    -   Orkestrasi async tidak boleh mengeksekusi kerja PHP yang blocking
        langsung di thread eksekutor Tokio.
    -   Penanganan kegagalan subsistem Rust dan shutdown graceful adalah
        syarat desain kelas-satu.

## ADR-0003: Arsitektur runtime PHP hibrida

-   **Status:** Accepted
-   **Keputusan:** PHPX mendukung dua backend runtime di balik abstraksi
    runtime bersama:
    1.  **Process Runtime:** PHP dieksekusi di proses OS terpisah yang
        terawasi. Ini default dan baseline produksi.
    2.  **Embedded Runtime:** PHP Embed SAPI dihosting melalui bridge nativ
        yang dikendalikan ketat. Ini tetap eksperimental hingga lolos
        gerbang kompatibilitas, isolasi, dan stabilitas.
-   **Konteks:** Eksekusi process memberi batas kompatibilitas dan
    isolasi-kesalahan yang lebih kuat. Embedding dapat mengurangi overhead
    batas dan menyederhanakan beberapa bentuk deployment, tetapi membawa
    kompleksitas ABI, lifecycle, thread-safety, ekstensi, dan containment crash.
-   **Konsekuensi:**
    -   Pemilihan runtime eksplisit di konfigurasi; tanpa peralihan backend diam-diam.
    -   Kedua backend mengimplementasikan kontrak lifecycle dan eksekusi
        yang menghadap platform yang sama di mana semantik benar-benar cocok.
    -   Kapabilitas spesifik-backend diiklankan, bukan diasumsikan.
    -   Eksekusi embedded harus memakai domain eksekusi terkendali. PHP
        NTS tidak boleh dimasuki konkuren dari thread Tokio sembarang.
    -   Batas process tetap tersedia untuk workload tak tepercaya atau
        sensitif-crash.
    -   Backend embedded tidak bisa menjadi default hanya karena sebuah
        microbenchmark lebih cepat.

## ADR-0004: Kompatibilitas aplikasi PHP yang ketat

-   **Status:** Accepted
-   **Keputusan:** PHPX menarget aplikasi PHP konvensional tanpa modifikasi.
    API spesifik-platform adalah peningkatan opsional dan tidak pernah
    disyaratkan untuk eksekusi baseline.
-   **Konteks:** Platform harus berguna bagi ekosistem PHP yang ada,
    terutama Laravel, WordPress, dan WooCommerce.
-   **Konsekuensi:**
    -   PHPX harus menjaga semantik request PHP normal, variabel
        environment, perilaku document-root/front-controller, header,
        cookie, body request, kode status, dan perilaku error.
    -   Kompatibilitas diverifikasi memakai fixture aplikasi asli dan
        matriks ekstensi.
    -   PHPX tidak menjanjikan kompatibilitas dengan setiap ekstensi atau
        setiap perilaku SAPI PHP di v0.1.
    -   Perilaku tak didukung harus didokumentasikan tepat; PHPX harus
        gagal jelas alih-alih diam-diam mengubah semantik aplikasi.
    -   API PHPX opsional butuh versionalisasi dan tidak bisa menjadi
        prasyarat hosting aplikasi biasa.

## ADR-0005: Docker dan Linux nativ adalah target deployment kelas-satu

-   **Status:** Accepted
-   **Keputusan:** PHPX mendukung instalasi Linux nativ maupun deployment
    terkontainerisasi. Tidak ada yang menjadi warga kelas-dua.
-   **Konteks:** Deployment nativ berguna untuk integrasi host langsung dan
    instalasi overhead-rendah. Kontainer sentral untuk deployment berulang
    dan orkestrasi.
-   **Konsekuensi:**
    -   Binary inti dan model konfigurasi yang sama harus bekerja di kedua
        lingkungan.
    -   Path filesystem, sinyal, perizinan, cgroup, dan supervisi proses
        harus diuji di keduanya.
    -   Image Docker harus mendukung eksekusi non-root secara default bila
        memungkinkan.
    -   Packaging nativ dan integrasi layanan (mis. systemd)
        didokumentasikan terpisah dari orkestrasi kontainer.
    -   Windows bukan target produksi v0.1; pengembangan Windows boleh
        memakai WSL2 atau kontainer.

## ADR-0006: Orkestrasi Tokio, eksekusi blocking terisolasi

-   **Status:** Accepted
-   **Keputusan:** Tokio dipakai untuk I/O platform dan orkestrasi async.
    Eksekusi PHP dan operasi blocking lain berjalan di luar thread worker
    Tokio.
-   **Konteks:** Eksekusi PHP bukan future Tokio async dan bisa memblokir
    selama durasi sembarang.
-   **Konsekuensi:**
    -   Worker process berkomunikasi lewat kanal IPC terbatas.
    -   PHP embedded, bila dipakai, berjalan di thread khusus terkendali
        atau domain eksekusi dengan akses terserial sesuai batasan build
        PHP dan SAPI.
    -   Backpressure dan antrean terbatas hukumnya wajib.
    -   Pembatalan request HTTP tidak menyiratkan interupsi tak aman
        terhadap interpreter PHP yang sedang mengeksekusi; penanganan
        timeout spesifik-backend dan terdokumentasi.

## ADR-0007: Implementasi gateway HTTP

-   **Status:** Accepted
-   **Keputusan:** Pakai Hyper dengan Tokio untuk gateway HTTP awal, dan
    Rustls untuk TLS.
-   **Konteks:** Komponen ini memberi fondasi HTTP Rust modular tanpa
    mengharuskan PHP memiliki network listener.
-   **Konsekuensi:**
    -   HTTP/1.1 wajib untuk v0.1; HTTP/2 adalah target yang tunduk pada
        integration testing.
    -   HTTP/3 di luar lingkup v0.1.
    -   Serving file statik, batas request, shutdown graceful, dan dispatch
        front-controller PHP adalah bagian gateway.
    -   Reverse proxy dan fitur edge lanjutan ditunda.

## ADR-0008: Format konfigurasi dan kompatibilitas

-   **Status:** Accepted
-   **Keputusan:** Pakai TOML untuk file konfigurasi utama yang ditulis
    manusia, dengan deserialisasi bertipe berbasis Serde dan validasi skema
    eksplisit.
-   **Konteks:** Operator butuh format konfigurasi yang terbaca dan dapat direview.
-   **Konsekuensi:**
    -   Konfigurasi punya versi skema eksplisit.
    -   Nilai kritis tak dikenal atau tak valid gagal cepat dengan error
        actionable.
    -   Default didokumentasikan dan stabil.
    -   Override environment-variable memakai konvensi penamaan
        terdokumentasi.
    -   Secret sebaiknya dipasok lewat environment variable atau secret
        mount, bukan di-commit ke file TOML.

## ADR-0009: Durabilitas antre dan adapter storage

-   **Status:** Accepted
-   **Keputusan:** Queue dan storage adalah antarmuka pluggable. SQLite
    adalah backend antre tahan-lama awal satu-node; Redis adalah adapter
    opsional. Antre in-memory hanya untuk pengembangan.
-   **Konteks:** Antre platform tidak boleh kehilangan job yang sudah
    di-acknowledge saat proses restart.
-   **Konsekuensi:**
    -   Transisi state job, retry, lease/visibility timeout, dan
        penanganan dead-letter butuh semantik transaksional.
    -   Driver in-memory ditandai eksplisit non-durable.
    -   PostgreSQL dan backend terdistribusi lain ditunda.
    -   Adapter storage filesystem dan kompatibel-S3 diperkenalkan
        bertahap.

## ADR-0010: Observabilitas bawaan

-   **Status:** Accepted
-   **Keputusan:** Log terstruktur, korelasi request, metrik, dan tracing
    adalah kapabilitas platform, bukan renungan opsional.
-   **Konteks:** Platform runtime harus dapat didiagnosis di bawah beban
    produksi nyata.
-   **Konsekuensi:**
    -   Pakai ekosistem Rust `tracing`.
    -   Ekspos metrik kompatibel-Prometheus.
    -   Tambah integrasi OpenTelemetry bertahap.
    -   Jangan pernah mencatat secret, header otorisasi, atau payload
        request sensitif secara default.
    -   Metrik runtime dan worker harus dapat diatribusikan ke aplikasi
        dan worker.

## ADR-0011: Keamanan dan isolasi resource

-   **Status:** Accepted
-   **Keputusan:** PHPX menerapkan batas resource eksplisit dan eksekusi
    least-privilege. Worker PHP tidak boleh mewarisi hak tak perlu dari
    supervisor platform.
-   **Konteks:** Aplikasi dan ekstensi PHP mengeksekusi kode aplikasi
    sembarang dan bisa terkompromi.
-   **Konsekuensi:**
    -   Konfigurasikan timeout eksekusi, batas memori, jumlah worker, ukuran
        request, dan batas antre.
    -   Dukung operasi non-root.
    -   Pisahkan hak supervisor dari hak worker aplikasi bila praktis.
    -   Dokumentasikan bahwa `memory_limit` PHP bukan batas memori level-OS
        yang lengkap.
    -   Batas container/cgroup dan sandboxing OS direkomendasikan untuk
        tenant tak tepercaya.
    -   Isolasi kode-bermusuhan multi-tenant tidak diklaim oleh v0.1.

## ADR-0012: Workspace Rust modular dan subsistem feature-gated

-   **Status:** Accepted
-   **Keputusan:** Pakai workspace Cargo dengan crate kecil yang
    diorganisasi di sekitar batas subsistem stabil. Fitur platform opsional
    dinyalakan lewat fitur Cargo dan konfigurasi runtime bila sesuai.
-   **Konteks:** Crate monolitik akan menyulitkan testing dan ownership;
    fragmentasi berlebih terlalu dini akan memperlambat pengembangan.
-   **Konsekuensi:**
    -   Mulai dengan set kecil crate fondasional dan belah hanya bila
        batasnya jelas.
    -   Hindari dependensi siklik.
    -   Jaga kontrak runtime di crate level-rendah.
    -   Adapter opsional tidak boleh menjadi dependensi wajib server minimal.

## ADR-0013: Kebijakan rilis dan kompatibilitas

-   **Status:** Accepted
-   **Keputusan:** PHPX mengikuti semantic versioning setelah rilis stabil
    pertamanya. Sebelum 1.0, konfigurasi dan CLI boleh berevolusi, tetapi
    perubahan breaking harus didokumentasikan.
-   **Konteks:** Operator butuh upgrade yang dapat diprediksi dan developer
    perlu memahami garansi kompatibilitas.
-   **Konsekuensi:**
    -   v0.1 adalah developer preview eksperimental, bukan janji dukungan
        produksi.
    -   Setiap rilis mempublikasikan matriks Rust, PHP, OS, dan ekstensi
        yang didukung.
    -   Perbedaan kapabilitas backend runtime didokumentasikan.
    -   Test kompatibilitas adalah gerbang rilis.

## ADR-0014: Siklus hidup worker, manajemen resource dan recycling adaptif (Proposed)

-   **Status:** Proposed (belum menjadi bagian baseline Accepted; sebagian
    besar isinya — lifecycle terawasi, guardian memori, autoscale adaptif —
    sudah terimplementasi di P0-B/P0-C/P2 dan dipin oleh test)
-   **Keputusan:** Tambahkan Runtime Health dan Memory Guardian ke Process
    Runtime. Siklus kanonis adalah `Starting -> Ready -> Busy -> Ready`,
    dengan `Ready/Busy -> Draining -> Stopped`,
    `Busy -> Unhealthy -> Restarting -> Ready`, dan
    `Starting -> Failed`. `Failed` terminal untuk inkarnasi worker itu
    dengan diagnostik yang terlihat operator. Lihat
    `adr/0014-worker-lifecycle-resource-management.md` untuk rasional penuh.
-   **Konteks:** Worker PHP persistent berisiko bocor state lintas-request
    dan tumbuh memori. Kontrol resource adalah diferensiator PHPX yang
    realistis, bukan speedup otomatis.
-   **Konsekuensi:**
    -   Pool terbatas + antre terbatas + backpressure tetap wajib.
        Default satu request konkuren per worker process.
    -   Ambang memori soft memicu recycle graceful; ambang hard
        memungkinkan terminasi protektif. Overshoot mungkin terjadi karena
        latensi sampling/terminasi dan harus didokumentasikan.
    -   Restart memakai exponential backoff + jitter + budget restart +
        circuit breaker agar tak ada loop respawn tak berujung.
    -   Kunci lifecycle/memori/adaptif detail hidup di bawah
        `[runtime.process.lifecycle]`, `[runtime.process.memory]`, dan
        `[runtime.process.adaptive]` (adaptif `enabled = false` secara
        default). Contoh SPEC Seksi 8 disederhanakan dan menunjuk ke sini
        untuk model penuh.
    -   Benchmark pekerjaan ini membandingkan dengan FrankenPHP, RoadRunner,
        Swoole/OpenSwoole, dan Nginx + PHP-FPM pada build identik dengan
        p50/p95/p99/max, RSS, restart, dan metrik recovery. Tanpa klaim
        paritas tanpa hasil berulang.
    -   Hingga diterima, SPEC Seksi 5 tetap normatif untuk baseline v0.1;
        ADR-0014 menyempurnakannya.

## Keputusan yang ditunda

Berikut ini sengaja tidak ditetapkan di v0.1:

-   Strategi ABI embedded PHP final dan varian build PHP yang didukung
    (pin ABI selesai — NTS non-debug 8.3-8.5; link nativ CI tetap ditunda).
-   Apakah PHP-CGI, protokol worker PHP CLI, atau FastCGI adalah transport
    process pilihan di setiap deployment.
-   Semantik queue terdistribusi dan driver PostgreSQL.
-   Dukungan nativ Windows.
-   HTTP/3.
-   Garansi sandbox kode-bermusuhan multi-tenant.
-   Stabilitas ABI plugin.

Keputusan ini butuh prototipe dan bukti, bukan asumsi.
