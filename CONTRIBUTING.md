# Contributing (M0)

- Small, testable PRs. M0 scope is the Rust workspace skeleton only.
- Run before push: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Config changes must keep `phpx.toml.example` parsing (see `phpx-config` tests).
- CLI changes must keep `phpx doctor` output actionable and secret-free.
- Docs: update SPEC/roadmap/ADR only via explicit ADR process; ADR-0014 is Proposed.
