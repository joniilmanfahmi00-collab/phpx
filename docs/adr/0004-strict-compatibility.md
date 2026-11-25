## ADR-0004: Kompatibilitas aplikasi PHP yang ketat

-   **Status:** Diterima (Accepted)
-   **Keputusan:** PHPX menargetkan aplikasi PHP konvensional yang tidak
    dimodifikasi. API khusus platform bersifat peningkatan opsional dan
    tidak pernah diwajibkan untuk eksekusi dasar.
-   **Konteks:** Platform ini harus berguna bagi ekosistem PHP yang ada,
    terutama Laravel, WordPress, dan WooCommerce.
-   **Konsekuensi:**
    -   PHPX harus mempertahankan semantik request PHP yang normal,
        variabel environment, perilaku document-root/front-controller,
        header, cookie, body request, kode status, dan perilaku error.
    -   Kompatibilitas diverifikasi menggunakan fixture aplikasi nyata
        dan matriks ekstensi.
    -   PHPX tidak menjanjikan kompatibilitas dengan setiap ekstensi
        atau setiap perilaku SAPI PHP pada v0.1.
    -   Perilaku yang tidak didukung harus didokumentasikan secara
        tepat; PHPX harus gagal secara jelas daripada diam-diam mengubah
        semantik aplikasi.
    -   API PHPX opsional wajib berversi dan tidak boleh menjadi syarat
        untuk hosting aplikasi biasa.
