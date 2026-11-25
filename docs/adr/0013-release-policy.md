## ADR-0013: Kebijakan rilis dan kompatibilitas

-   **Status:** Diterima (Accepted)
-   **Keputusan:** PHPX mengikuti semantic versioning setelah rilis stabil
    pertamanya. Sebelum 1.0, konfigurasi dan CLI dapat berkembang, tetapi
    perubahan yang merusak (breaking changes) harus didokumentasikan.
-   **Konteks:** Operator membutuhkan upgrade yang dapat diprediksi dan
    pengembang perlu memahami jaminan kompatibilitas.
-   **Konsekuensi:**
    -   v0.1 adalah pratinjau pengembang yang eksperimental, bukan janji
        dukungan produksi.
    -   Setiap rilis mempublikasikan matriks Rust, PHP, OS, dan ekstensi
        yang didukung.
    -   Perbedaan kapabilitas backend runtime didokumentasikan.
    -   Uji kompatibilitas adalah gerbang rilis.
