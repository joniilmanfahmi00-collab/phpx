## ADR-0006: Orkestrasi Tokio, eksekusi blocking yang terisolasi

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Tokio digunakan untuk I/O platform yang asinkron dan
    orkestrasi. Eksekusi PHP dan operasi blocking lainnya berjalan di
    luar thread worker Tokio.
-   **Konteks:** Eksekusi PHP bukan async Tokio future dan dapat
    memblokir selama durasi sembarang.
-   **Konsekuensi:**
    -   Worker proses berkomunikasi melalui kanal IPC yang berbatas
        (bounded).
    -   PHP embedded, bila digunakan, berjalan dalam thread khusus yang
        terkendali atau domain eksekusi dengan akses terserialisasi
        sesuai batasan build PHP dan SAPI.
    -   Backpressure dan antrean berbatas bersifat wajib.
    -   Pembatalan suatu request HTTP tidak berarti penghentian paksa
        yang tidak aman terhadap interpreter PHP yang sedang
        dieksekusi; penanganan timeout bersifat khusus backend dan
        didokumentasikan.
