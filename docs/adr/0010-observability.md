## ADR-0010: Observabilitas dibangun di dalam platform

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Log terstruktur, korelasi request, metrik, dan tracing
    adalah kapabilitas platform, bukan tambahan opsional.
-   **Konteks:** Sebuah platform runtime harus dapat didiagnosis di bawah
    beban produksi yang nyata.
-   **Konsekuensi:**
    -   Gunakan ekosistem `tracing` Rust.
    -   Ekspos metrik yang kompatibel dengan Prometheus.
    -   Tambahkan integrasi OpenTelemetry secara bertahap.
    -   Jangan pernah mencatat secret, header otorisasi, atau payload
        request yang sensitif secara default.
    -   Metrik runtime dan worker harus dapat diatribusikan ke suatu
        aplikasi dan worker.
