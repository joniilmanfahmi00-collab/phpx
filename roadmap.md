# Roadmap PHPX

**Versi:** 0.1.0\
**Status:** Rencana implementasi (M0-M7 selesai; P0-P2 selesai; soak 6 jam terarsip 2026-11-25)\
**Tanggal:** 2026-11-17 (diperbarui 2026-11-25)

PHPX dikembangkan dalam milestone berbasis bukti. Roadmap diurutkan
berdasarkan dependensi dan risiko, bukan jumlah fitur yang terlihat. Setiap
fase punya gerbang keluar konkret.

## Aturan pedoman

1.  Process Runtime adalah backend eksekusi pertama yang mampu produksi.
2.  Embedded Runtime eksperimental hingga lolos gerbang kompatibilitas dan
    stabilitas.
3.  Kompatibilitas ketat berarti aplikasi PHP biasa tidak butuh perubahan sumber.
4.  Linux nativ dan Docker sama-sama target deployment kelas-satu.
5.  Tanpa pool worker, antre request, atau antre job yang tak terbatas.
6.  Jangan optimasi sebelum benchmark baseline yang dapat diulang ada.
7.  Jaga server minimal tetap dapat dipakai tanpa layanan queue, cache, atau
    scheduler opsional.

## Milestone 0: Fondasi repositori dan engineering

**Tujuan:** Membuat workspace Rust yang dapat dibangun dan diuji.

### Kerja

-   [x] Buat repositori dan file tata kelola proyek.
-   [x] Tambah workspace `Cargo.toml` dan `rust-toolchain.toml` yang di-pin.
-   [x] Tegakkan CI formatting, linting, dan test.
-   [x] Definisikan konvensi error dan logging terstruktur.
-   [x] Definisikan skema konfigurasi versi 1.
-   [x] Implementasikan kerangka `phpx --version` dan `phpx doctor`.
-   [x] Dokumentasikan prasyarat pengembangan Linux.
-   [x] Tambah gambaran arsitektur dan ADR yang diterima.

### Kriteria keluar

-   [x] CI membangun dan mengetes pada target Linux yang didukung.
-   [x] `cargo fmt --check` dan Clippy lolos.
-   [x] `phpx doctor` melaporkan prasyarat Rust/PHP dengan jelas.
-   [x] Contoh konfigurasi ter-parse dan tervalidasi.

## Milestone 1: Prototipe Process Runtime

**Tujuan:** Mengeksekusi skrip PHP lewat worker yang disupervisi Rust.

### Kerja

-   [x] Temukan executable PHP dan inspeksi versi.
-   [x] Inspeksi ekstensi PHP yang dibutuhkan.
-   [x] Definisikan envelope request/response berversi.
-   [x] Prototipe IPC Unix Domain Socket berbingkai.
-   [x] Implementasikan bootstrap worker PHP.
-   [x] Start dan stop worker anak.
-   [x] Tangkap stdout/stderr dan status keluar dengan aman.
-   [x] Tambah timeout startup dan eksekusi.
-   [x] Tambah health worker dan kebijakan restart.
-   [x] Tambah fixture PHP vanilla sederhana.

### Kriteria keluar

-   [x] Rust bisa men-start worker dan mengeksekusi request PHP.
-   [x] Request konkuren dibatasi.
-   [x] Crash worker tidak menjatuhkan supervisor.
-   [x] Tidak ada data request yang menyeberang antar request berurutan.
-   [x] Integration test lolos pada minimal satu versi PHP yang didukung.

## Milestone 2: HTTP gateway

**Tujuan:** Melayani aplikasi PHP konvensional lewat PHPX.

### Kerja

-   [x] Tambah listener Tokio + Hyper.
-   [x] Tambah penanganan request HTTP/1.1.
-   [x] Tambah konfigurasi host/aplikasi.
-   [x] Tambah routing document-root dan front-controller.
-   [x] Tambah serving file statik yang aman.
-   [x] Tambah batas body dan header request.
-   [x] Tambah dispatch request PHP.
-   [x] Tambah ID request dan access log.
-   [x] Tambah shutdown graceful dan draining worker.
-   [x] Tambah TLS via Rustls.
-   [x] Validasi dukungan HTTP/2 sebelum mengklaimnya.

### Kriteria keluar

-   [x] App PHP vanilla bekerja tanpa modifikasi sumber.
-   [x] Aset statik dan front controller PHP bekerja.
-   [x] Path tak valid tidak bisa keluar document root.
-   [x] Gateway tetap tersedia melewati crash worker PHP.
-   [x] Backpressure mencegah request tertunda tak terbatas.

## Milestone 3: Developer preview v0.1

**Tujuan:** Mengirimkan server aplikasi PHP minimal yang berguna.

### Kerja

-   [x] Finalisasi `phpx serve`.
-   [x] Finalisasi `phpx doctor`.
-   [x] Tambah perintah status runtime dan worker.
-   [x] Tambah konfigurasi TOML dan default terdokumentasi.
-   [x] Tambah image Docker yang jalan non-root bila memungkinkan.
-   [x] Tambah instruksi instalasi Linux nativ.
-   [x] Tambah contoh/panduan systemd.
-   [x] Publikasikan matriks versi PHP yang didukung.
-   [x] Tambah smoke test dan benchmark baseline yang dapat diulang.

### Kriteria keluar

-   [x] Quickstart Docker lolos.
-   [x] Quickstart Linux nativ lolos.
-   [x] Fixture kompatibilitas PHP vanilla lolos.
-   [x] Keterbatasan operasional didokumentasikan.
-   [x] v0.1 diberi label jelas developer preview, bukan SLA produksi.

Keterlusuran: butir penerimaan SPEC Seksi 15 nomor 1-4, 8-10 tercakup M1-M3; nomor 5-7 plus 11-12 diselesaikan M4. Queue/scheduler/embedded (ekor SPEC 15) eksplisit ditunda ke M5-M6 dan tidak boleh memblokir M3.

## Milestone 4: Hardening worker dan kompatibilitas aplikasi

**Tujuan:** Membuat Process Runtime tangguh untuk aplikasi nyata.

### Kerja

-   [x] Recycling worker per jumlah request dan health.
-   [x] Dokumentasikan state worker Failed vs Stopped per SPEC 5.1 + penyelarasan ADR-0014.
-   [x] Ukuran pool min/max yang dapat dikonfigurasi.
-   [x] Backoff startup dan proteksi crash-loop.
-   [x] Semantik timeout request dan drain graceful.
-   [x] Audit environment PHP dan kompatibilitas SAPI.
-   [x] Fixture Composer.
-   [x] Fixture kompatibilitas Laravel.
-   [x] Fixture kompatibilitas WordPress.
-   [x] Smoke test WooCommerce setelah baseline WordPress.
-   [x] Test kombinasi ekstensi PHP.
-   [x] Soak test dan analisis pertumbuhan memori.

### Kriteria keluar

-   [x] Tidak ada kontaminasi silang request yang diketahui.
-   [x] Crash recovery dan reload graceful lolos.
-   [x] Matriks kompatibilitas dipublikasikan.
-   [x] Perilaku SAPI/ekstensi tak didukung eksplisit.

## Milestone 5: Layanan platform

**Tujuan:** Menambah infrastruktur aplikasi yang berguna tanpa mengorbankan
server inti. — DONE (queue tahan lama sqlite/redis, scheduler cron sadar-timezone, cache memory/filesystem/redis, storage lokal nyata + prototipe s3).

### Kriteria keluar

-   [x] Job queue tahan lama selamat dari restart.
-   [x] Perilaku retry dan dead-letter teruji.
-   [x] Scheduler tidak mengeksekusi PHP di thread async gateway.
-   [x] Adapter cache dan storage punya contract test.

## Milestone 6: Jalur eksperimental Embedded Runtime

**Tujuan:** Memvalidasi backend runtime kedua tanpa melemahkan
Process Runtime. — DONE hingga serve path (P2): 9/9 gerbang promosi lolos
pada build linked (`gates::assess`, `available=true`); `EmbeddedPool`
single-flight + timeout eksplisit terkabel ke gateway
(`runtime.mode=embedded`); stub fail-closed; default tetap `process`.
Link nativ di CI tetap ditunda (butuh header php-dev di runner).

### Kriteria keluar

-   [x] Backend embedded opt-in dan bertanda jelas eksperimental.
-   [x] Kapabilitas yang diklaim lolos contract test bersama.
-   [x] Tidak ada entri engine PHP konkuren yang sembarang.
-   [x] Batasan dan matriks build yang didukung dipublikasikan.
-   [x] Process Runtime tetap tersedia sebagai fallback, dipilih eksplisit
    via konfigurasi.

## Milestone 7: Kandidat produksi

**Tujuan:** Menyiapkan PHPX untuk evaluasi produksi yang serius. — DONE
(security review, non-root, cgroup, traversal/symlink, redaksi secret, bench
+ soak, OTel-ready, metrik Prometheus, upgrade/rollback, hardening Docker +
SBOM, otomasi rilis, backup/recovery). Catatan: ini kesiapan evaluasi,
bukan klaim SLA — lihat `release-gates.md`.

## Milestone lanjutan: remediasi engineering P0-P2 + soak panjang (DONE 2026-11-25)

- **P0**: mode eksekusi eksplisit, `PersistentPool` terawasi, guardian RSS +
  `memory_limit`, app asli Laravel 12 + WordPress 7.1 + WooCommerce 11.3.
- **P1**: matriks resiliensi 9 test + ambang penerimaan, bench lab + repeat +
  CV gate, multipart `$_POST`/`$_FILES`, driver Redis queue/cache, pool
  adaptif opt-in.
- **P2**: serve path embedded terkabel (pooling + routing + timeout eksplisit,
  `p2_embedded_gateway` 5/5 di stub dan linked), observabilitas CLI melaporkan
  pool embedded.
- **Soak**: arsip 60 menit (2026-11-23, 12.600/12.600, +3,6 KB/mnt) dan
  **6 jam (2026-11-25, 73.600/73.600, gagal 0, slope -0,86 KB/mnt)** —
  lihat `docs/operations/soak.md`.
- Sisa menuju RC: rekor RC di hardware RC, SLO rps/p95/slope formal, tren 24 jam.

## Urutan yang disarankan

    Order Milestone                          Prioritas
  ------- ---------------------------------- -----------------------------
        0 Fondasi repositori                 P0
        1 Process Runtime                    P0
        2 HTTP gateway                       P0
        3 Developer preview v0.1             P0
        4 Kompatibilitas worker dan app      P0
        5 Queue, scheduler, cache, storage   P1
        6 Embedded Runtime                   P1
        7 Hardening produksi                 P0 sebelum klaim produksi

Embedded Runtime boleh diprototipe paralel setelah kontrak runtime ada, tetapi
tidak boleh memblokir rilis Process Runtime.

## 10 tugas implementasi pertama

1.  Inisialisasi workspace Rust dan CI.
2.  Implementasikan tipe error dan konfigurasi bersama.
3.  Implementasikan discovery executable PHP dan `phpx doctor`.
4.  Definisikan protokol request/response runtime.
5.  Bangun bootstrap worker PHP minimal.
6.  Implementasikan spawn, supervisi, dan shutdown process.
7.  Eksekusi request PHP vanilla lewat IPC.
8.  Tambah gateway Hyper dan routing front-controller.
9.  Tambah pool worker terbatas dan backpressure.
10. Ship quickstart Docker dan Linux nativ.

## Definisi selesai untuk setiap fitur

-   [ ] Unit test meliput logika inti.
-   [ ] Integration test meliput batas subsistem.
-   [ ] Perilaku gagal didefinisikan.
-   [ ] Pemakaian resource dibatasi.
-   [ ] Log dan diagnostik actionable.
-   [ ] Default konfigurasi dan kompatibilitas didokumentasikan.
-   [ ] Implikasi keamanan direview.
-   [ ] Fitur nonaktif atau gagal aman bila tak tersedia.
