# Kontrak worker persistent (P0-A didefinisikan, P0-B terimplementasi)

Status: **Terimplementasi.** `execution_mode = "persistent"` memilih pool terawasi berukuran-tetap (`PersistentPool`); `execution_mode = "one-shot"` (default)
mempertahankan throttle one-shot. Keduanya dilaporkan via CLI/JSON dan
`phpx_execution_mode_info`.

## 1. Mode

| Mode | Arti | Status |
|------|---------|--------|
| `one-shot` | Satu proses `php` baru per request yang diterima (`execute_one_shot`). Tanpa state lintas-request secara konstruksi. | Terimplementasi (M1, dikeraskan M4) |
| `persistent` | Pool tetap berisi worker PHP berumur-panjang yang terawasi dengan identitas, handshake, recycling. | Terimplementasi (P0-B) |

## 2. Identitas

- `worker_id: u64` — indeks slot stabil antar restart (lihat `WorkerIdentity` di `phpx-runtime`).
- `incarnation: u64` — naik setiap kali proses OS untuk sebuah slot di-(re)spawn.
- `pid: Option<u32>` — handle proses OS yang hidup saat diawasi; `None` sebelum spawn / setelah reaping.
- Counter recycle logis saja (`recycled` saat ini) **bukan** recycle proses;
  P0-B wajib mengikat recycling ke incarnation baru + PID baru.

## 3. Transport dan protokol

- Envelope: JSON `PhpRequest` / `PhpResponse` dengan `protocol_version` (saat ini `1`, lihat `phpx-runtime::PROTOCOL_VERSION`).
- Framing: 4-byte big-endian length + JSON (`encode_frame` / `decode_frame` / `write_frame`).
  Transport prototipe adalah stdio pipes; transport Unix-socket hadir bersama P0-B.
- Perubahan envelope yang breaking menaikkan `PROTOCOL_VERSION`; supervisor dan worker wajib
  sepakat, jika tidak request gagal dengan error protokol alih-alih dieksekusi.
- Eksekusi PHP yang blocking **tidak pernah** berjalan di thread eksekutor Tokio: gateway
  mendispatch via `tokio::task::spawn_blocking` (ADR-0006). P0-B mempertahankan aturan ini
  untuk handshake, pompa I/O, dan supervision waits.

## 4. Siklus hidup (P0-B terimplementasi; dipin oleh test lifecycle)

`Starting -> Ready -> Busy -> Ready`, plus `Ready -> Draining -> Stopped`,
`Busy -> Draining -> Stopped`, `Busy -> Unhealthy -> Restarting -> Ready`,
`Starting -> Failed`. Pengganti menjadi ready hanya setelah handshake
startup yang sukses di dalam `startup_timeout_seconds`.

## 5. Aplikasi mana yang bisa memakai mode persistent

Belum ada selain fixture — mode persistent dapat dipilih sejak P0-B dan
kelayakan dimulai dari vanilla PHP stateless (terbukti: reuse, pemulihan crash,
timeout, recycle, reset SAPI), lalu aplikasi berbasis Composer (terbukti: reset SAPI
antar reuse), lalu suite smoke Laravel / WordPress nyata (pekerjaan P0-D, tidak
diklaim). Aplikasi
yang mengandalkan in-memory state lintas-request, `$_SESSION` tanpa storage eksternal,
streaming `$_FILES`, atau replay `php://input` wajib tetap di one-shot sampai
aturan reset-state di bawah terbukti untuk mereka.

## 6. Limitasi reset-state

Worker PHP persistent tidak boleh membocorkan state antar request. Sampai terbukti
sebaliknya, anggap kebocoran via: superglobals, variabel statis, state opcache
per-proses, handle terbuka, working directory, locale/timezone, state RNG,
dan efek-samping shutdown-function. Kontrak reset (P0-B + P1-B, terimplementasi di
`worker-persistent.php`): `$_GET`/`$_POST` (urlencoded diurai seperti SAPI web;
multipart tidak diurai)/`$_COOKIE`/`$_REQUEST` dibangun ulang per
request, `HTTP_*` basi dibuang, `PHPX_*` diekspor ulang per request, output
buffering direset, `phpx_input()` disajikan dari body frame. Variabel statis
dan `$GLOBALS` yang diset oleh kode aplikasi secara sengaja persist (batas terdokumentasi: pakai
one-shot bila isolasi ketat diwajibkan).

## 7. Guardian sumber daya (P0-C, terimplementasi)

RSS OS per-worker disampel di dua titik: sinkron saat request
selesai, dan oleh thread sampler latar setiap
`memory_sample_interval_seconds` untuk worker idle (worker Busy tidak pernah
diblokir oleh sampler). Observasi per incarnation: `rss_bytes`,
`peak_rss_bytes` (maks tersampel, batas-bawah dari peak sejati), `healthy`,
`last_ok_at`, plus `startup_duration_ms` (spawn+handshake) dan
`shutdown_duration_ms` (kill+wait) dari transisi siklus hidup.

Titik penegakan terpisah (jangan dicampuradukkan):

- `memory_limit` (PHP `-d memory_limit`): hanya Zend heap, kedua mode.
- `memory_soft_limit` (OS RSS): recycle graceful, pengganti dulu.
- `memory_hard_limit` (OS RSS): terminasi protektif + respawn.

Soft < hard bila keduanya diset (`"0"` menonaktifkan). Pemicu dihitung per worker
(`phpx_worker_memory_limit_hits_total{threshold}`) dan restart membawa
penyebab `soft-limit`/`hard-limit`. Overshoot antar titik sampel dimungkinkan
(berbasis sampling, bukan cap keras); RSS != heap PHP, dan baik RSS stabil maupun
tinggi tidak membuktikan apa pun tentang leak — guardian mendaur-ulang dan melindungi,
ia tidak mendiagnosis.
