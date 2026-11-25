## ADR-0011: Keamanan dan isolasi sumber daya

-   **Status:** Diterima (Accepted)
-   **Keputusan:** PHPX menerapkan batas sumber daya yang eksplisit dan
    eksekusi dengan hak istimewa minimal (least-privilege). Worker PHP
    tidak boleh mewarisi hak istimewa yang tidak perlu dari supervisor
    platform.
-   **Konteks:** Aplikasi dan ekstensi PHP mengeksekusi kode aplikasi
    sembarang dan dapat dikompromikan.
-   **Konsekuensi:**
    -   Konfigurasikan timeout eksekusi, batas memori, jumlah worker,
        ukuran request, dan batas antrean.
    -   Dukung operasi non-root.
    -   Pisahkan hak istimewa supervisor dari hak istimewa worker
        aplikasi sejauh praktis.
    -   Dokumentasikan bahwa `memory_limit` PHP bukan batas memori
        setingkat OS yang utuh.
    -   Batas kontainer/cgroup dan sandboxing OS direkomendasikan untuk
        tenant yang tidak tepercaya.
    -   Isolasi kode bermusuhan multi-tenant tidak diklaim oleh v0.1.
