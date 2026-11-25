## ADR-0003: Arsitektur runtime PHP hibrida

-   **Status:** Diterima (Accepted)
-   **Keputusan:** PHPX mendukung dua backend runtime di balik abstraksi
    runtime yang sama:
    1.  **Process Runtime:** PHP dieksekusi dalam proses OS terpisah
        yang disupervisi. Ini adalah default dan basis produksi.
    2.  **Embedded Runtime:** PHP Embed SAPI dihos melalui native
        bridge yang dikendalikan secara cermat. Backend ini tetap
        eksperimental sampai lolos gerbang kompatibilitas, isolasi, dan
        stabilitas.
-   **Konteks:** Eksekusi berbasis proses menyediakan batas
    kompatibilitas dan isolasi kesalahan yang lebih kuat. Embedding
    dapat mengurangi overhead batas dan menyederhanakan sebagian bentuk
    deployment, tetapi menimbulkan kompleksitas ABI, siklus hidup,
    thread-safety, ekstensi, dan penjinakan crash.
-   **Konsekuensi:**
    -   Pemilihan runtime bersifat eksplisit dalam konfigurasi; tidak
        ada peralihan backend diam-diam.
    -   Kedua backend mengimplementasikan kontrak siklus hidup dan
        eksekusi yang sama di sisi platform sejauh semantiknya
        benar-benar cocok.
    -   Kapabilitas khusus backend diiklankan (advertised), bukan
        diasumsikan.
    -   Eksekusi embedded harus menggunakan domain eksekusi yang
        terkendali. PHP NTS tidak boleh dimasuki secara konkuren dari
        thread Tokio sembarang.
    -   Batas proses tetap tersedia untuk beban kerja yang tidak
        tepercaya atau sensitif terhadap crash.
    -   Backend embedded tidak dapat menjadi default hanya karena sebuah
        microbenchmark lebih cepat.
