# ADR-0014: Siklus Hidup Worker, Manajemen Sumber Daya & Recycling Adaptif

- **Status:** Diusulkan (Proposed)
- **Tanggal:** 2026-11-17
- **Lingkup:** Process Runtime, dengan persyaratan kompatibilitas untuk Embedded Runtime mendatang.
- **Bahasa:** Isi utama dalam bahasa Indonesia (diterjemahkan dari bahasa Sunda; naskah asli berbahasa Sunda). Abstrak bahasa Inggris di bawah bersifat normatif untuk keselarasan antar-dokumen.
- **Menggantikan / Memperluas:** Memperluas SPEC Bagian 5 (manajemen Worker) dan Bagian 8 (Konfigurasi). Tidak menggantikan SPEC. Di mana SPEC mencantumkan Failed, itu dipetakan ke Stopped dengan diagnostic reason + restart-budget habis dalam ADR ini.

> **Abstrak bahasa Inggris (penyelarasan normatif; diterjemahkan ke bahasa Indonesia):** PHPX mengimplementasikan Runtime Health and Memory Guardian sebagai bagian dari Process Runtime. Worker mengikuti Starting -> Ready -> Busy -> Ready, dengan Ready/Busy -> Draining -> Stopped, Busy -> Unhealthy -> Restarting -> Ready, dan Starting -> Failed. Failed bersifat terminal untuk inkarnasi worker tersebut dan membutuhkan diagnostik yang terlihat oleh operator. Bounded pool + bounded queue + backpressure bersifat wajib. Default-nya adalah satu request konkuren per process worker. Memori menggunakan ambang soft (recycle graceful) dan hard (terminasi protektif). Penskalaan adaptif ditunda (nonaktif secara default) hingga baseline fixed-pool stabil. Lihat SPEC Bagian 5, 8, 11 serta ADR-0003, ADR-0004, ADR-0006 untuk kontrak dasar.

## 1. Konteks

PHPX dirancang sebagai Rust-powered PHP Application Platform. Diferensiasi PHPX tidak boleh bergantung pada asumsi bahwa Rust secara otomatis membuat kode aplikasi PHP lebih cepat.

Keunggulan yang lebih realistis adalah kemampuan mengelola sumber daya PHP secara konsisten, meliputi:

- Bounded concurrency dan explicit backpressure.
- Worker health monitoring.
- Memory-aware worker recycling.
- Graceful shutdown dan deployment.
- Crash recovery.
- Resource utilization monitoring.
- Runtime lifecycle orchestration.

Persistent PHP worker dapat mengurangi biaya application bootstrap, tetapi juga membawa risiko persistensi state antar-request. Global variables, static properties, extension state, dan in-memory cache dapat tetap hidup selama worker masih aktif.

Oleh karena itu, PHPX membutuhkan mekanisme untuk mengawasi, mengatur, dan mengembalikan worker ke keadaan sehat tanpa mengganggu aplikasi secara tidak perlu.

## 2. Keputusan

PHPX akan mengembangkan Runtime Health & Memory Guardian sebagai bagian integral dari Process Runtime.

Subsistem ini bertanggung jawab untuk mengawasi keadaan worker, menganalisis pemakaian sumber daya, serta mengoordinasikan tindakan siklus hidup melalui Worker Manager.

PHPX akan menggunakan pendekatan manajemen sumber daya yang konservatif pada rilis awal. Prioritasnya adalah perilaku yang dapat diprediksi, stabilitas, dan observabilitas sebelum optimasi adaptif.

### 2.1 State Machine Siklus Hidup Worker

Setiap worker harus memiliki lifecycle state yang jelas:

- `Starting`: proses sedang dibuat dan diinisialisasi.
- `Ready`: worker sudah siap menerima request.
- `Busy`: worker sedang mengolah satu request atau job.
- `Draining`: worker tidak menerima request baru, tetapi boleh menyelesaikan tugas yang sedang berjalan.
- `Restarting`: worker sedang diganti.
- `Unhealthy`: worker gagal dalam health check atau melampaui batas operasional.
- `Stopped`: worker sudah berhenti dan tidak termasuk kapasitas aktif.
- `Failed`: worker gagal startup atau melampaui restart budget / circuit breaker. Terminal untuk inkarnasi tersebut, wajib ada diagnostic reason. Pemetaan dengan SPEC Bagian 5.1 Starting -> Failed.

Worker yang berada dalam status `Draining`, `Restarting`, `Unhealthy`, `Stopped`, atau `Failed` tidak boleh menerima tugas baru.

### 2.2 Bounded Execution & Backpressure

PHPX wajib menggunakan bounded worker pool dan bounded request queue.

Konkuren default untuk setiap PHP process worker adalah satu request dalam satu waktu.

Eksekusi PHP tidak boleh dijalankan langsung di atas Tokio executor thread karena operasi PHP umumnya blocking.

Bila kapasitas worker sudah penuh, PHPX harus menerapkan backpressure serta overload response yang dikonfigurasi, alih-alih membiarkan request queue tumbuh tanpa batas.

Queue waiting time dan request execution time harus diukur secara terpisah.

PHPX tidak mengklaim menyediakan coroutine concurrency dalam satu PHP worker. Pemrosesan request paralel dalam Process Runtime awalnya dicapai melalui beberapa worker yang diawasi.

### 2.3 Worker Health Monitoring

Runtime Health & Memory Guardian harus mengumpulkan informasi:

- Worker RSS dan peak RSS bila tersedia.
- Worker uptime.
- Jumlah request yang sudah selesai.
- Durasi eksekusi request.
- Durasi startup dan shutdown worker.
- Exit code dan termination reason.
- Restart count dan frekuensi restart.
- Health check terakhir yang berhasil.
- Status worker dan beban kerja saat ini.

Pemakaian memori setingkat PHP harus dibedakan dari RSS setingkat OS.

RSS tidak otomatis membuktikan memory leak. Alokator PHP, ekstensi native, shared library, dan fragmentasi memori dapat menjaga RSS tetap tinggi walaupun objek aplikasi sudah dilepaskan.

### 2.4 Kebijakan Recycling Worker

Worker dapat di-recycle berdasarkan beberapa kondisi:

- Jumlah request selesai maksimum.
- Ambang memori soft.
- Ambang memori hard.
- Uptime worker maksimum.
- Timeout request berulang.
- Kegagalan health check.
- Tindakan administratif manual.
- Penggantian deployment.

Proses graceful recycling:

1. Worker dipindahkan ke status `Draining`.
2. Worker tidak boleh menerima request baru.
3. Request yang sedang berjalan diberi waktu untuk selesai.
4. Runtime mengirimkan graceful shutdown signal.
5. Worker dipaksa berhenti bila shutdown deadline terlewati.
6. Replacement worker dibuat.
7. Worker baru menjadi `Ready` setelah startup handshake berhasil.

PHPX harus menghindari simultaneous worker restart yang dapat mengurangi kapasitas layanan secara drastis.

### 2.5 Manajemen Ambang Memori

PHPX akan membedakan dua ambang memori.

**Ambang Memori Soft**

Bila worker mencapai batas ini, PHPX akan menjadwalkan graceful recycling setelah request saat ini selesai.

**Ambang Memori Hard**

Bila worker mencapai batas ini, PHPX dapat melakukan protective termination untuk menjaga stabilitas host.

Ambang memori harus dapat dikonfigurasi per worker, baik dalam ukuran absolut maupun persentase memori host bila platform mendukung.

PHPX tidak menjamin bahwa konsumsi memori tidak akan pernah melampaui nilai konfigurasi. Sampling interval, allocation burst, dan termination latency dapat menyebabkan overshoot.

### 2.6 Proteksi Restart

Unexpected worker exit harus ditangani dengan mekanisme:

- Exponential backoff.
- Randomized jitter.
- Restart budget dalam rolling time window.
- Circuit breaker untuk repeated startup failure.
- Status diagnostik bila restart budget habis.

Tujuannya adalah mencegah infinite respawn loop yang dapat menyebabkan konsumsi CPU dan memori tanpa kendali.

### 2.7 Manajemen Worker Adaptif

Penskalaan worker adaptif ditunda hingga fixed worker pool memiliki baseline yang stabil.

Pada tahap berikutnya, PHPX dapat menyesuaikan jumlah worker berdasarkan:

- Queue depth.
- Queue waiting time.
- Jumlah worker aktif.
- Tekanan CPU.
- Headroom memori.
- Ketersediaan sumber daya sistem.

Adaptive controller harus memiliki:

- Batas worker minimum dan maksimum.
- Ambang scale-up dan scale-down.
- Cooldown period.
- Hysteresis.
- Safeguard memori setingkat host.
- Kapasitas worker minimum-ready.

Mode adaptif harus nonaktif secara default hingga hasil benchmark membuktikan bahwa mekanisme ini aman dan memberi manfaat nyata.

### 2.8 Isolasi Status Request

PHPX harus menjelaskan bahwa persistent PHP worker dapat menjaga process-level state antar-request.

Runtime harus menyediakan request lifecycle boundary serta mengevaluasi framework-specific reset hooks bila tersedia.

PHPX tidak boleh menjamin isolasi state universal untuk seluruh aplikasi PHP konvensional.

Untuk aplikasi yang tidak aman dijalankan dalam persistent worker, PHPX harus menyediakan mode eksekusi kompatibilitas yang membatasi reuse proses, termasuk one-request-per-process bila diperlukan.

Pengujian kompatibilitas harus mencakup:

- Global variables.
- Static properties.
- Session state.
- Environment variables.
- Output buffer.
- Error handler.
- Extension-managed state.

### 2.9 Model Konfigurasi

Contoh konfigurasi awal:

```toml
[runtime.process]
workers = 4
max_concurrent_requests_per_worker = 1

[runtime.process.lifecycle]
max_requests = 1000
max_uptime = "1h"
drain_timeout = "10s"
shutdown_timeout = "5s"
restart_backoff = "250ms"
restart_backoff_max = "30s"
restart_budget = 5
restart_window = "60s"

[runtime.process.memory]
enabled = true
check_interval = "5s"
soft_limit = "256MB"
hard_limit = "384MB"
recycle_on_soft_limit = true
terminate_on_hard_limit = true

[runtime.process.adaptive]
enabled = false
min_workers = 2
max_workers = 16
scale_up_queue_depth = 8
scale_down_idle_duration = "60s"
cooldown = "15s"
```

Nilai di atas merupakan contoh konfigurasi, bukan default final. Nilai default harus ditentukan melalui benchmark dan validasi pada deployment profile yang didukung.

### 2.10 Observabilitas

PHPX harus menyediakan metrik dan structured event untuk:

- Distribusi status worker.
- Worker RSS dan peak RSS.
- Request selesai per worker.
- Jumlah restart worker berdasarkan alasan.
- Kegagalan restart.
- Kedalaman antrean dan durasi tunggu antrean.
- Jumlah overload response.
- Durasi eksekusi request.
- Durasi startup dan shutdown worker.

Structured log tidak boleh mengandung secret, request body, isi sesi, atau data sensitif secara default.

## 3. Konsekuensi

### Konsekuensi Positif

- Utilisasi sumber daya lebih mudah dikontrol dan diprediksi.
- Pertumbuhan memori dapat ditangani melalui recycling terkendali.
- Crash worker memiliki mekanisme pemulihan yang jelas.
- Backpressure mengurangi risiko pertumbuhan antrean tanpa batas.
- Operator dapat melihat alasan worker di-recycle atau diterminasi.
- PHPX memiliki diferensiasi di tingkat platform tanpa harus membuat klaim performa yang belum terbukti.

### Konsekuensi Negatif

- Supervisi worker dan pemantauan sumber daya menambah kompleksitas implementasi.
- Sampling memori memiliki overhead.
- Recycling worker dapat menambah latensi akibat bootstrap aplikasi ulang.
- Isolasi state persistent worker tidak dapat dijamin untuk semua aplikasi.
- Batas sumber daya yang konservatif dapat mengurangi throughput puncak.
- Penskalaan adaptif membutuhkan tuning yang cermat untuk menghindari osilasi.

## 4. Rencana Implementasi

### Fase A: Runtime Proses Deterministik

1. Membangun state machine siklus hidup worker.
2. Mengimplementasikan bounded worker pool dan batas antrean.
3. Menambahkan penghitung request dan recycling berbasis request maksimum.
4. Mengimplementasikan graceful drain dan shutdown deadline.
5. Menambahkan deteksi crash dan proteksi restart.
6. Menyediakan metrik siklus hidup worker.

### Fase B: Supervisi Sadar Memori

1. Membangun abstraksi sampling RSS.
2. Mengimplementasikan ambang memori soft dan hard.
3. Menambahkan graceful recycling berdasarkan ambang soft.
4. Menambahkan protective termination berdasarkan ambang hard.
5. Menguji kegagalan sampling dan overshoot ambang.
6. Mendokumentasikan batasan pemantauan RSS.

### Fase C: Kompatibilitas & Stabilitas

1. Menguji Vanilla PHP, Laravel, WordPress, dan WooCommerce.
2. Menguji kebocoran state antar-request.
3. Melakukan pengujian stabilitas memori durasi panjang.
4. Menguji crash worker, timeout, terminasi OOM, dan deployment graceful.
5. Memvalidasi Linux native dan Docker.

### Fase D: Manajemen Adaptif

1. Membangun prototipe penskalaan sadar antrean dan sadar memori.
2. Menerapkan hysteresis dan cooldown.
3. Membandingkan manajemen adaptif dengan fixed worker pool.
4. Menjaga mode adaptif nonaktif hingga ada bukti stabilitas yang cukup.

## 5. Kriteria Validasi & Penerimaan

Runtime Health & Memory Guardian tidak dianggap production-ready sebelum:

- Transisi siklus hidup worker deterministik dan sudah diuji.
- Jumlah worker dan ukuran antrean selalu berada dalam batas konfigurasi.
- Worker yang draining tidak menerima request baru.
- Worker exit yang tak terduga tidak menyebabkan infinite restart loop.
- Ambang memori soft memicu recycling terkendali.
- Ambang memori hard dapat menjaga host dalam batas sampling dan latensi terminasi yang didokumentasikan.
- Metrik dapat menjelaskan alasan setiap worker di-recycle atau diterminasi.
- Uji kompatibilitas request berulang lolos.
- Uji durasi panjang menghasilkan laporan tren RSS, jumlah restart, throughput, dan latensi.
- Perilaku Linux native dan Docker sudah divalidasi.

Benchmark PHPX harus dibandingkan dengan FrankenPHP, RoadRunner, Swoole/OpenSwoole, serta Nginx + PHP-FPM bila relevan.

Seluruh benchmark harus menggunakan hardware, build PHP, versi PHP, ekstensi, aplikasi, data store, workload, dan metodologi tuning yang sama.

Laporan benchmark harus mencakup:

- Request per detik.
- Latensi p50, p95, p99, dan maksimum.
- RSS per worker dan total.
- Pertumbuhan memori dari waktu ke waktu.
- Jumlah dan alasan restart worker.
- Utilisasi CPU.
- Kedalaman antrean dan perilaku overload.
- Waktu pemulihan setelah kegagalan worker.

PHPX tidak boleh memublikasikan klaim performance parity sebelum ada hasil benchmark yang repeatable.

## 6. Alternatif yang Dipertimbangkan

### Recycling Berbasis Jumlah Request Saja

Ditolak sebagai satu-satunya mekanisme karena jumlah request tidak cukup untuk menggambarkan pertumbuhan memori dan kesehatan worker.

### Ambang Memori Saja

Ditolak karena tidak dapat mendeteksi seluruh masalah, termasuk worker macet, kegagalan startup berulang, dan kebocoran state.

### Restart Worker Tersinkronisasi Tetap

Ditolak karena dapat menyebabkan kapasitas layanan turun sekaligus dan bootstrap aplikasi yang tidak perlu.

### Penskalaan Adaptif Agresif pada Rilis Awal

Ditunda karena PHPX membutuhkan baseline fixed-pool yang stabil sebelum membangun adaptive controller.

### Sepenuhnya Mengandalkan OOM Killer OS atau Kontainer

Ditolak sebagai strategi utama karena OOM killer merupakan mekanisme perlindungan terakhir, bukan manajemen siklus hidup setingkat aplikasi.

## 7. Referensi

- Spesifikasi Teknis PHPX v0.1.
- ADR-0003: Hybrid PHP Runtime.
- ADR-0004: Strict Compatibility.
- ADR-0006: Tokio and Blocking Execution.
- ADR-0010: Observability.
- ADR-0011: Security and Isolation.
- Dokumentasi FrankenPHP Worker: https://frankenphp.dev/docs/worker/
- Konfigurasi Proses OpenSwoole: https://openswoole.com/docs/modules/swoole-server/configuration
