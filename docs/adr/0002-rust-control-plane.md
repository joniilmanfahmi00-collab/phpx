## ADR-0002: Rust adalah bidang kendali (control plane) platform

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Rust memegang infrastruktur platform: gateway,
    manajemen siklus hidup, supervisi worker, penjadwalan, koordinasi
    antrean, kendali sumber daya, observabilitas, dan CLI.
-   **Konteks:** Rust menawarkan kendali sumber daya yang dapat
    diprediksi serta kapabilitas pemrograman sistem yang kuat. PHP
    tetap menjadi bahasa aplikasi dan mesin eksekusi.
-   **Konsekuensi:**
    -   Kode aplikasi PHP tidak ditulis ulang dalam Rust.
    -   Batas antara platform dan runtime bersifat eksplisit dan
        berversi.
    -   Orkestrasi async tidak boleh mengeksekusi kerja PHP yang
        bersifat blocking langsung di atas thread eksekutor Tokio.
    -   Penanganan kegagalan subsistem Rust dan graceful shutdown
        merupakan kebutuhan desain kelas utama.
