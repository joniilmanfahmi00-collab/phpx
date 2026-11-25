# Berkontribusi

- PR kecil yang dapat diuji. Lingkup M0 hanya kerangka workspace Rust
  (kini: ikuti roadmap — M0-M7 + P0-P2 selesai; lihat `roadmap.md`).
- Jalankan sebelum push: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Perubahan config harus menjaga `phpx.toml.example` tetap ter-parse (lihat test `phpx-config`).
- Perubahan CLI harus menjaga output `phpx doctor` tetap actionable dan bebas secret.
- Dok: perbarui SPEC/roadmap/ADR hanya via proses ADR eksplisit; ADR-0014 berstatus Proposed.
- Bahasa dok: Indonesia (dok lama Inggris/Sunda diterjemahkan bertahap).
