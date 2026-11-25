## ADR-0008: Format dan kompatibilitas konfigurasi

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Gunakan TOML untuk file konfigurasi utama yang ditulis
    manusia, dengan deserialisasi bertipe berbasis Serde dan validasi
    skema yang eksplisit.
-   **Konteks:** Operator membutuhkan format konfigurasi yang mudah
    dibaca dan ditelaah.
-   **Konsekuensi:**
    -   Konfigurasi memiliki versi skema yang eksplisit.
    -   Nilai kritis yang tidak dikenal atau tidak valid gagal cepat
        (fail fast) dengan error yang dapat ditindaklanjuti.
    -   Nilai default didokumentasikan dan stabil.
    -   Override melalui variabel environment menggunakan konvensi
        penamaan yang didokumentasikan.
    -   Secret sebaiknya dipasok melalui variabel environment atau
        secret mount, bukan di-commit ke file TOML.
