## ADR-0007: Implementasi HTTP gateway

-   **Status:** Diterima (Accepted)
-   **Keputusan:** Gunakan Hyper bersama Tokio untuk HTTP gateway awal,
    dan Rustls untuk TLS.
-   **Konteks:** Komponen-komponen ini menyediakan fondasi HTTP Rust yang
    modular tanpa mengharuskan PHP memegang network listener.
-   **Konsekuensi:**
    -   HTTP/1.1 wajib untuk v0.1; HTTP/2 adalah target yang tunduk pada
        pengujian integrasi.
    -   HTTP/3 di luar lingkup v0.1.
    -   Penyajian file statis, batas request, graceful shutdown, dan
        dispatch front-controller PHP adalah bagian dari gateway.
    -   Reverse proxy dan fitur edge lanjutan ditunda.
