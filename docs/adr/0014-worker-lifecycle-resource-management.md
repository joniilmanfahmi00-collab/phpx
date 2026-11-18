# ADR-0014: Worker Lifecycle, Resource Management & Adaptive Recycling

- **Status:** Proposed
- **Date:** 2026-11-17
- **Scope:** Process Runtime, with compatibility requirements for future Embedded Runtime.
- **Language:** Main body in Sundanese. English abstract below is normative for cross-document alignment.
- **Supersedes / Extends:** Extends SPEC Section 5 (Worker management) and Section 8 (Configuration). Does not replace SPEC. Where SPEC lists Failed, that maps to Stopped with diagnostic reason + restart-budget exhausted in this ADR.

> **English abstract (normative alignment):** PHPX implements a Runtime Health and Memory Guardian as part of Process Runtime. Workers follow Starting -> Ready -> Busy -> Ready, with Ready/Busy -> Draining -> Stopped, Busy -> Unhealthy -> Restarting -> Ready, and Starting -> Failed. Failed is terminal for that worker incarnation and requires operator-visible diagnostics. Bounded pool + bounded queue + backpressure are mandatory. Default is one concurrent request per process worker. Memory uses soft (graceful recycle) and hard (protective termination) thresholds. Adaptive scaling is deferred (disabled by default) until a fixed-pool baseline is stable. See SPEC Sections 5, 8, 11 and ADR-0003, ADR-0004, ADR-0006 for base contracts.

## 1. Context

PHPX dirancang minangka Rust-powered PHP Application Platform. Diferensiasi PHPX henteu kedah gumantung kana asumsi yén Rust sacara otomatis ngajadikeun kode aplikasi PHP leuwih gancang.

Kaunggulan anu leuwih realistis nyaéta kamampuh ngatur sumber daya PHP sacara konsisten, ngawengku:

- Bounded concurrency jeung explicit backpressure.
- Worker health monitoring.
- Memory-aware worker recycling.
- Graceful shutdown jeung deployment.
- Crash recovery.
- Resource utilization monitoring.
- Runtime lifecycle orchestration.

Persistent PHP workers tiasa ngurangan biaya application bootstrap, tapi ogé miboga résiko state persistence antar-request. Global variables, static properties, extension state, jeung in-memory caches tiasa tetep hirup salila worker masih aktip.

Ku kituna, PHPX peryogi mékanisme pikeun ngawas, ngatur, jeung mulangkeun worker kana kaayaan séhat tanpa ngaganggu aplikasi sacara teu perlu.

## 2. Decision

PHPX bakal ngembangkeun Runtime Health & Memory Guardian minangka bagian integral tina Process Runtime.

Subsystem ieu tanggung jawab pikeun ngawas kaayaan worker, nganalisis pamakéan sumber daya, sarta koordinasi tindakan lifecycle ngaliwatan Worker Manager.

PHPX bakal ngagunakeun pendekatan resource management anu konservatif dina release awal. Prioritasna nyaéta predictable behavior, stability, jeung observability saméméh adaptive optimization.

### 2.1 Worker Lifecycle State Machine

Unggal worker kudu miboga lifecycle state anu jelas:

- `Starting`: prosés keur dijieun jeung diinisialisasi.
- `Ready`: worker geus siap narima request.
- `Busy`: worker keur ngolah hiji request atawa job.
- `Draining`: worker henteu narima request anyar, tapi meunang ngaréngsékeun tugas anu keur lumangsung.
- `Restarting`: worker keur diganti.
- `Unhealthy`: worker gagal dina health check atawa ngaleuwihan wates operasional.
- `Stopped`: worker geus eureun sarta henteu kaasup kapasitas aktif.
- `Failed`: worker gagal startup atawa ngaleuwihan restart budget / circuit breaker. Terminal pikeun inkarnasi eta, wajib aya diagnostic reason. Pemetaan sareng SPEC Section 5.1 Starting -> Failed.

Worker anu aya dina status `Draining`, `Restarting`, `Unhealthy`, `Stopped`, atawa `Failed` henteu meunang narima tugas anyar.

### 2.2 Bounded Execution & Backpressure

PHPX wajib ngagunakeun bounded worker pool jeung bounded request queue.

Default concurrency pikeun unggal PHP process worker nyaéta hiji request dina hiji waktu.

PHP execution henteu meunang dijalankeun langsung dina Tokio executor thread sabab operasi PHP umumna blocking.

Lamun kapasitas worker geus pinuh, PHPX kudu nerapkeun backpressure sarta overload response anu dikonfigurasi, tibatan ngantepkeun request queue tumuwuh tanpa wates.

Queue waiting time jeung request execution time kudu diukur sacara misah.

PHPX henteu ngaku nyadiakeun coroutine concurrency dina hiji PHP worker. Parallel request processing dina Process Runtime awalna kahontal ngaliwatan sababaraha worker anu diawasan.

### 2.3 Worker Health Monitoring

Runtime Health & Memory Guardian kudu ngumpulkeun informasi:

- Worker RSS jeung peak RSS lamun sadia.
- Worker uptime.
- Jumlah request anu geus réngsé.
- Request execution duration.
- Worker startup jeung shutdown duration.
- Exit code jeung termination reason.
- Restart count jeung restart frequency.
- Last successful health check.
- Worker state jeung workload ayeuna.

PHP-level memory usage kudu dibédakeun ti OS-level RSS.

RSS henteu otomatis ngabuktikeun memory leak. PHP allocator, native extensions, shared libraries, jeung memory fragmentation tiasa ngajaga RSS tetep luhur sanajan objék aplikasi geus dileupaskeun.

### 2.4 Worker Recycling Policy

Worker tiasa direcycle dumasar kana sababaraha kaayaan:

- Maximum completed requests.
- Soft memory threshold.
- Hard memory threshold.
- Maximum worker uptime.
- Repeated request timeout.
- Health check failure.
- Manual administrative action.
- Deployment replacement.

Prosés graceful recycling:

1. Worker dipindahkeun kana status `Draining`.
2. Worker henteu meunang narima request anyar.
3. Request anu keur lumangsung dibéré waktos pikeun réngsé.
4. Runtime ngirimkeun graceful shutdown signal.
5. Worker dipaksa eureun lamun shutdown deadline kaliwat.
6. Replacement worker dijieun.
7. Worker anyar jadi `Ready` sanggeus startup handshake hasil.

PHPX kudu nyingkahan simultaneous worker restart anu tiasa ngurangan kapasitas layanan sacara drastis.

### 2.5 Memory Threshold Management

PHPX bakal ngabédakeun dua memory threshold.

**Soft Memory Threshold**

Lamun worker ngahontal wates ieu, PHPX bakal ngajadwalkeun graceful recycling sanggeus request ayeuna réngsé.

**Hard Memory Threshold**

Lamun worker ngahontal wates ieu, PHPX tiasa ngalakukeun protective termination pikeun ngajaga stabilitas host.

Memory threshold kudu bisa dikonfigurasi per worker, boh dina ukuran absolut boh persentase mémori host lamun platform ngadukung.

PHPX henteu ngajamin yén konsumsi mémori moal kungsi ngaleuwihan nilai konfigurasi. Sampling interval, allocation burst, jeung termination latency tiasa nyababkeun overshoot.

### 2.6 Restart Protection

Unexpected worker exit kudu ditangani ku mékanisme:

- Exponential backoff.
- Randomized jitter.
- Restart budget dina rolling time window.
- Circuit breaker pikeun repeated startup failure.
- Diagnostic status lamun restart budget béak.

Tujuanana nyaéta nyegah infinite respawn loop anu tiasa ngabalukarkeun konsumsi CPU jeung mémori tanpa kendali.

### 2.7 Adaptive Worker Management

Adaptive worker scaling ditunda nepi ka fixed worker pool boga baseline anu stabil.

Dina tahap salajengna, PHPX tiasa nyaluyukeun jumlah worker dumasar kana:

- Queue depth.
- Queue waiting time.
- Active worker count.
- CPU pressure.
- Memory headroom.
- System resource availability.

Adaptive controller kudu miboga:

- Minimum jeung maximum worker limits.
- Scale-up jeung scale-down thresholds.
- Cooldown period.
- Hysteresis.
- Host-level memory safeguards.
- Minimum-ready worker capacity.

Adaptive mode kudu nonaktif sacara default nepi ka hasil benchmark ngabuktikeun yén mékanisme ieu aman jeung méré mangpaat nyata.

### 2.8 Request State Isolation

PHPX kudu ngajelaskeun yén persistent PHP workers tiasa ngajaga process-level state antar-request.

Runtime kudu nyadiakeun request lifecycle boundary sarta ngaevaluasi framework-specific reset hooks lamun sadia.

PHPX henteu meunang ngajamin universal state isolation pikeun sakabéh aplikasi PHP konvensional.

Pikeun aplikasi anu henteu aman dijalankeun dina persistent worker, PHPX kudu nyadiakeun compatibility execution mode anu ngawatesan process reuse, kaasup one-request-per-process lamun diperlukeun.

Compatibility testing kudu ngawengku:

- Global variables.
- Static properties.
- Session state.
- Environment variables.
- Output buffers.
- Error handlers.
- Extension-managed state.

### 2.9 Configuration Model

Conto konfigurasi awal:

```toml
[runtime.process]
workers = 4
max_concurrent_requests_per_worker = 1

[runtime.process.lifecycle]
max_requests = 1000
max_uptime = "1h"
drain_timeout = "10s"
shutdown_timeout = "5s"
restart_backoff = "250ms"
restart_backoff_max = "30s"
restart_budget = 5
restart_window = "60s"

[runtime.process.memory]
enabled = true
check_interval = "5s"
soft_limit = "256MB"
hard_limit = "384MB"
recycle_on_soft_limit = true
terminate_on_hard_limit = true

[runtime.process.adaptive]
enabled = false
min_workers = 2
max_workers = 16
scale_up_queue_depth = 8
scale_down_idle_duration = "60s"
cooldown = "15s"
```

Nilai di luhur mangrupa conto konfigurasi, sanés default final. Default kudu ditangtukeun ngaliwatan benchmark jeung validasi dina deployment profile anu dirojong.

### 2.10 Observability

PHPX kudu nyadiakeun metrics jeung structured events pikeun:

- Worker state distribution.
- Worker RSS jeung peak RSS.
- Requests completed per worker.
- Worker restart count dumasar kana alesan.
- Restart failure.
- Queue depth jeung queue waiting duration.
- Overload response count.
- Request execution duration.
- Worker startup jeung shutdown duration.

Structured logs henteu meunang ngandung secrets, request bodies, session contents, atawa data sénsitip sacara default.

## 3. Consequences

### Positive Consequences

- Resource utilization leuwih gampang dikontrol jeung diprediksi.
- Memory growth tiasa ditangani ngaliwatan controlled recycling.
- Worker crash miboga recovery mechanism anu jelas.
- Backpressure ngurangan résiko unbounded queue growth.
- Operator tiasa ningali alesan worker direcycle atawa diterminasi.
- PHPX miboga diferensiasi dina tingkat platform tanpa kudu nyieun klaim performa anu can kabuktian.

### Negative Consequences

- Worker supervision jeung resource monitoring nambahan kompleksitas implementasi.
- Memory sampling miboga overhead.
- Worker recycling tiasa nambahan latency alatan application bootstrap ulang.
- Persistent worker state isolation henteu tiasa dijamin pikeun sakabéh aplikasi.
- Resource limit anu konservatif tiasa ngurangan peak throughput.
- Adaptive scaling peryogi tuning anu taliti pikeun nyingkahan oscillation.

## 4. Implementation Plan

### Phase A: Deterministic Process Runtime

1. Ngawangun worker lifecycle state machine.
2. Ngimplementasikeun bounded worker pool jeung queue limits.
3. Nambahkeun request counter jeung maximum-request recycling.
4. Ngimplementasikeun graceful drain jeung shutdown deadline.
5. Nambahkeun crash detection jeung restart protection.
6. Nyadiakeun worker lifecycle metrics.

### Phase B: Memory-aware Supervision

1. Ngawangun RSS sampling abstraction.
2. Ngimplementasikeun soft jeung hard memory thresholds.
3. Nambahkeun graceful recycling dumasar kana soft threshold.
4. Nambahkeun protective termination dumasar kana hard threshold.
5. Nguji sampling failure jeung threshold overshoot.
6. Ngadokumentasikeun watesan RSS monitoring.

### Phase C: Compatibility & Stability

1. Nguji Vanilla PHP, Laravel, WordPress, jeung WooCommerce.
2. Nguji state leakage antar-request.
3. Ngalakukeun long-duration memory stability testing.
4. Nguji worker crash, timeout, OOM termination, jeung graceful deployment.
5. Validasi native Linux jeung Docker.

### Phase D: Adaptive Management

1. Ngawangun prototype queue-aware jeung memory-aware scaling.
2. Nerapkeun hysteresis jeung cooldown.
3. Ngabandingkeun adaptive management jeung fixed worker pool.
4. Ngajaga adaptive mode nonaktif nepi ka aya bukti stabilitas anu cekap.

## 5. Validation & Acceptance Criteria

Runtime Health & Memory Guardian henteu dianggap production-ready saméméh:

- Worker lifecycle transitions deterministik jeung geus diuji.
- Worker count jeung queue size salawasna aya dina configured bounds.
- Draining worker henteu narima request anyar.
- Unexpected worker exit henteu nyababkeun infinite restart loop.
- Soft memory threshold memicu controlled recycling.
- Hard memory threshold tiasa ngajaga host dina wates sampling jeung termination latency anu geus didokumentasikeun.
- Metrics tiasa ngajelaskeun alesan unggal worker direcycle atawa diterminasi.
- Repeated-request compatibility tests lulus.
- Long-duration tests ngahasilkeun laporan RSS trend, restart count, throughput, jeung latency.
- Native Linux jeung Docker behavior geus divalidasi.

Benchmark PHPX kudu dibandingkeun jeung FrankenPHP, RoadRunner, Swoole/OpenSwoole, sarta Nginx + PHP-FPM lamun relevan.

Sadaya benchmark kudu ngagunakeun hardware, PHP build, PHP version, extensions, application, data store, workload, jeung tuning methodology anu sarua.

Benchmark report kudu ngawengku:

- Requests per second.
- Latency p50, p95, p99, jeung maximum.
- Per-worker jeung total RSS.
- Memory growth over time.
- Worker restart count jeung reasons.
- CPU utilization.
- Queue depth jeung overload behavior.
- Recovery time sanggeus worker failure.

PHPX henteu meunang nyebarkeun klaim performance parity saméméh aya hasil benchmark anu repeatable.

## 6. Alternatives Considered

### Request-count Recycling Only

Ditolak minangka hiji-hijina mékanisme sabab jumlah request henteu cukup pikeun ngagambarkeun memory growth jeung worker health.

### Memory Threshold Only

Ditolak sabab henteu tiasa ngadeteksi sakabéh masalah, kaasup stuck worker, repeated startup failure, jeung state leakage.

### Fixed Synchronized Worker Restart

Ditolak sabab tiasa nyababkeun kapasitas layanan turun sacara sakaligus jeung application bootstrap anu teu perlu.

### Aggressive Adaptive Scaling in Initial Release

Ditunda sabab PHPX peryogi fixed-pool baseline anu stabil saméméh ngawangun adaptive controller.

### Rely Entirely on OS or Container OOM Killer

Ditolak minangka strategi utama sabab OOM killer mangrupa mékanisme panyalindungan pamungkas, sanés application-level lifecycle management.

## 7. References

- PHPX Technical Specification v0.1.
- ADR-0003: Hybrid PHP Runtime.
- ADR-0004: Strict Compatibility.
- ADR-0006: Tokio and Blocking Execution.
- ADR-0010: Observability.
- ADR-0011: Security and Isolation.
- FrankenPHP Worker Documentation: https://frankenphp.dev/docs/worker/
- OpenSwoole Process Configuration: https://openswoole.com/docs/modules/swoole-server/configuration
