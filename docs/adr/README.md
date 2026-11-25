# Architecture Decision Records

Keputusan arsitektur yang Diterima untuk PHPX v0.1 (ADR-0001 hingga
ADR-0013), ditambah satu usulan ekstensi:

-   [ADR-0001: Identitas dan lingkup
    produk](0001-product-identity-and-scope.md)
-   [ADR-0002: Rust adalah bidang kendali
    platform](0002-rust-control-plane.md)
-   [ADR-0003: Arsitektur runtime PHP
    hibrida](0003-hybrid-php-runtime.md)
-   [ADR-0004: Kompatibilitas aplikasi PHP yang
    ketat](0004-strict-compatibility.md)
-   [ADR-0005: Deployment Docker dan Linux
    native](0005-docker-and-native-linux.md)
-   [ADR-0006: Orkestrasi Tokio dan eksekusi
    blocking](0006-tokio-and-blocking-execution.md)
-   [ADR-0007: Implementasi HTTP gateway](0007-http-gateway.md)
-   [ADR-0008: Konfigurasi TOML](0008-toml-configuration.md)
-   [ADR-0009: Durabilitas antrean dan adapter](0009-queue-durability.md)
-   [ADR-0010: Observabilitas bawaan](0010-observability.md)
-   [ADR-0011: Keamanan dan isolasi sumber
    daya](0011-security-and-isolation.md)
-   [ADR-0012: Workspace Rust modular](0012-modular-workspace.md)
-   [ADR-0013: Kebijakan rilis dan kompatibilitas](0013-release-policy.md)
-   [ADR-0014: Siklus hidup worker, manajemen sumber daya, dan
    recycling adaptif](0014-worker-lifecycle-resource-management.md) —
    Status: Diusulkan (Proposed). Memperluas SPEC Bagian 5 dan Bagian 8
    dengan siklus hidup, ambang memori soft/hard, dan penskalaan adaptif
    yang ditunda. Isi utama dalam bahasa Indonesia (diterjemahkan dari
    bahasa Sunda) dengan kepala abstrak bahasa Inggris.

Lihat [semua keputusan dalam satu dokumen](../ARCHITECTURE_DECISIONS.md)
untuk konteks dan konsekuensi. Ringkasan ADR-0014 disertakan di sana
sebagai Diusulkan (Proposed), belum menjadi bagian dari basis Diterima.
