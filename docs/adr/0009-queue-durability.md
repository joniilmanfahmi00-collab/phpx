## ADR-0009: Durabilitas antrean dan adapter penyimpanan

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Antrean dan penyimpanan adalah antarmuka yang dapat
    dipasang-lepas (pluggable). SQLite adalah backend antrean durable
    awal untuk single-node; Redis adalah adapter opsional. Antrean
    in-memory hanya untuk pengembangan.
-   **Konteks:** Antrean platform tidak boleh kehilangan job yang sudah
    di-acknowledge saat proses di-restart.
-   **Konsekuensi:**
    -   Transisi status job, retry, lease/visibility timeout, dan
        penanganan dead-letter membutuhkan semantik transaksional.
    -   Driver in-memory ditandai secara eksplisit sebagai non-durable.
    -   PostgreSQL dan backend terdistribusi lainnya ditunda.
    -   Adapter penyimpanan filesystem dan yang kompatibel dengan S3
        diperkenalkan secara bertahap.
