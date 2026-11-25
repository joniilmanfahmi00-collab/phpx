# Tinjauan keamanan + model ancaman PHPX (M7 Fase 1)

**Status:** gerbang RC, pratinjau developer v0.1. Kode aplikasi + ekstensi PHP dianggap tepercaya (SPEC 10). Tanpa klaim isolasi multi-tenant.

## Batas kepercayaan

| # | Batas | Kepercayaan | Mitigasi (terverifikasi) |
|---|----------|-------|-----------------------|
| 1 | Jaringan -> gateway | Tidak tepercaya | Body terbatas (`request_body_limit`), pool + antre terbatas (503 + `Retry-After: 1`), timeout, TLS via Rustls opsional |
| 2 | Path URI -> FS | Tidak tepercaya | Tolak pra-FS (`..`, `\\`, NUL, `%2e/%2f/%5c/%00` -> 400) + cek kanonis `starts_with` docroot (escape -> 403). Tidak pernah sajikan `.php` mentah. |
| 3 | Header/log | Secret tidak tepercaya | Access log hanya `request_id/method/path/status/duration_ms`. `is_sensitive_header()` + `redacted_header_value()` menjamin: `authorization/cookie/*token*/*secret*/*password*/*api-key*/*auth*` -> `[redacted]`. Nilai penuh hanya masuk ke env PHP, tidak pernah ke log. |
| 4 | Worker PHP | App tepercaya, input tidak tepercaya | One-shot `php <entry>` per request, tanpa global lintas-request. Crash (`exit != 0`) -> 500, gateway tetap menyala. Anggaran restart + circuit breaker. |
| 5 | Config/file | Operator tepercaya | `phpx serve --config` tervalidasi; path PEM TLS, keberadaan docroot/entrypoint dicek. Tanpa secret di output `phpx doctor`/`metrics`. |
| 6 | Container/host | Tanggung jawab operator | Docker `USER phpx (10001)`, `HEALTHCHECK /healthz`, systemd `User=phpx + NoNewPrivileges + ProtectSystem=strict`. Batas CPU/memori cgroup via systemd/Docker (lihat bawah). |

## Verifikasi non-root (M7)

- Statis: `m7_dockerfile_runs_non_root` menegaskan `USER phpx` + `HEALTHCHECK` + uid `10001`.
- Runtime: image berjalan sebagai `phpx`; contoh systemd berjalan sebagai `User=phpx`.
- Cek operator: `docker run --rm phpx:0.1.0-dev whoami` wajib mencetak `phpx`, bukan `root`.

## Batas cgroup / container (operator)

M7 tidak mengklaim deteksi-otomatis batas cgroup; ia tetap terbatas dan teramati agar orchestrator Anda bisa menegakkannya:

```bash
# Docker: 2 CPU + 1GiB, gateway 503 saat jenuh alih-alih spiral OOM
docker run --rm -p 8080:8080 --cpus=2 --memory=1g phpx:0.1.0-dev

# systemd: lihat packaging/systemd/phpx.service (contoh MemoryMax=/CPUQuota= di upgrade-rollback.md)
```

Sinyal saturasi: `/metrics` (`rejected_overload`, `circuit_breaks`, `waiting`) + access log `status=503`.

## Aturan penanganan secret (gerbang M7)

1. Jangan pernah me-log nilai header; hanya method/path/status/durasi + ID.
2. Kode baru yang wajib menampilkan header wajib memanggil `redacted_header_value(name, value)`.
3. `traceparent` (W3C) adalah passthrough untuk OTel (bukan PII); isinya ID, aman di-log sebagai ID.
4. Templat laporan: dampak, batas terdampak di atas, repro (raw socket vs reqwest), perbaiki + nama uji regresi.

## Non-tujuan yang diketahui (RC eksplisit)

- Tanpa paritas WAF/CDN, tanpa klaim sandbox-escape untuk ekstensi berbahaya, tanpa enkripsi queue saat-rest (perm file SQLite adalah tanggung jawab operator, lihat backup-recovery.md).
