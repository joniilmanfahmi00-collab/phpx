## ADR-0012: Workspace Rust modular dan subsistem bergerbang fitur

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Gunakan Cargo workspace dengan crate kecil yang
    diorganisasikan di sekitar batas subsistem yang stabil. Fitur
    platform opsional diaktifkan melalui Cargo features dan konfigurasi
    runtime sebagaimana mestinya.
-   **Konteks:** Satu crate monolitik akan menyulitkan pengujian dan
    kepemilikan; fragmentasi berlebihan yang terlalu dini akan
    memperlambat pengembangan.
-   **Konsekuensi:**
    -   Mulai dengan himpunan kecil crate fondasional dan pecah hanya
        bila batasnya sudah jelas.
    -   Hindari dependensi siklik.
    -   Jaga kontrak runtime dalam crate tingkat rendah.
    -   Adapter opsional tidak boleh menjadi dependensi wajib bagi
        server minimal.
