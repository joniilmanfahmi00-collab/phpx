## ADR-0001: Identitas dan lingkup produk

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Produk ini bernama **PHPX** dan merupakan platform
    aplikasi PHP yang didukung Rust, bukan sekadar server HTTP atau
    manajer proses PHP.
-   **Konteks:** Lingkup yang hanya sebatas server akan membuat
    pemrosesan antrean, penjadwalan, siklus hidup runtime, integrasi
    penyimpanan, dan perkakas operasional menjadi terfragmentasi.
-   **Konsekuensi:**
    -   PHPX menyediakan CLI dan model konfigurasi yang terpadu bagi
        pengembang.
    -   Platform ini terdiri dari subsistem yang dapat diuji secara
        independen.
    -   Rilis pertama memprioritaskan jalur eksekusi HTTP + PHP yang
        andal; layanan lain ditambahkan secara bertahap.
    -   Nama PHPX adalah identitas produk sementara dan harus melalui
        pemeriksaan repositori, paket, domain, dan merek dagang sebelum
        rilis komersial.
