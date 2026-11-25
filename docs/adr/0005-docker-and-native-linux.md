## ADR-0005: Docker dan Linux native adalah target deployment kelas utama

-   **Status:** Diterima (Accepted)
-   **Keputusan:** PHPX mendukung instalasi Linux native maupun
    deployment dalam kontainer. Keduanya bukan pilihan kelas dua.
-   **Konteks:** Deployment native berguna untuk integrasi host langsung
    dan instalasi dengan overhead rendah. Kontainer menjadi pusat
    deployment yang dapat diulang dan orkestrasi.
-   **Konsekuensi:**
    -   Biner inti dan model konfigurasi yang sama seharusnya bekerja di
        kedua lingkungan.
    -   Path filesystem, sinyal, perizinan, cgroup, dan supervisi proses
        harus diuji di keduanya.
    -   Image Docker harus mendukung eksekusi non-root secara default
        sejauh memungkinkan.
    -   Pengemasan native dan integrasi layanan (misalnya systemd)
        didokumentasikan terpisah dari orkestrasi kontainer.
    -   Windows bukan target produksi v0.1; pengembangan di Windows
        dapat menggunakan WSL2 atau kontainer.
