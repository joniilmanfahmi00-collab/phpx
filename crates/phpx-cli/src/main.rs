use clap::{Parser, Subcommand};
use phpx_config::PhpxConfig;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser)]
#[command(
    name = "phpx",
    version,
    about = "Rust-powered PHP application platform (v0.1 developer preview, not for production SLA)"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Path to phpx.toml (default: ./phpx.toml, fallback phpx.toml.example)"
    )]
    config: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Emit machine-readable JSON where supported"
    )]
    json: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the gateway (M3: Tokio + Hyper HTTP/1.1, PHP dispatch, graceful drain, /healthz + /readyz).
    Serve {
        #[arg(long, help = "Override [server] host, e.g. 127.0.0.1")]
        host: Option<String>,
        #[arg(long, help = "Override [server] port, e.g. 8080")]
        port: Option<u16>,
    },
    /// Report Rust/PHP prerequisites clearly (M3 finalized).
    Doctor,
    /// Print version (SPEC 9: phpx version).
    Version,
    /// Runtime backends (M3: process active, embedded experimental disabled).
    #[command(name = "runtime")]
    Runtime {
        #[command(subcommand)]
        cmd: RuntimeCmd,
    },
    /// Worker pool inspection and reload guidance (M3).
    #[command(name = "workers")]
    Workers {
        #[command(subcommand)]
        cmd: WorkersCmd,
    },
    /// Queue surface (M5 phase 1 real: status/push/work).
    #[command(name = "queue")]
    Queue {
        #[command(subcommand)]
        cmd: QueueCmd,
    },
    /// Scheduler surface (M5 phase 2 real: single-pass dispatch).
    #[command(name = "scheduler")]
    Scheduler {
        #[command(subcommand)]
        cmd: SchedulerCmd,
    },
    /// Cache surface (M5 phase 3 real: memory + filesystem).
    #[command(name = "cache")]
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },
    /// Storage surface (M5 phase 3 real: local + s3 prototype).
    #[command(name = "storage")]
    Storage {
        #[command(subcommand)]
        cmd: StorageCmd,
    },
    /// Metrics snapshot (M4: access logs + health endpoints + Prometheus /metrics).
    Metrics,
}

#[derive(Debug, Subcommand)]
enum RuntimeCmd {
    /// List available runtimes and which one is active.
    List,
    /// Show active runtime details (mode, PHP target, pool bounds, docroot, TLS).
    Inspect,
}

#[derive(Debug, Subcommand)]
enum WorkersCmd {
    /// Show bounded pool configuration (min/max workers, queue bound, timeouts, restart policy).
    Status,
    /// Validate config and explain how reload works (stateless gateway, graceful drain).
    Reload,
}

#[derive(Debug, Subcommand)]
enum QueueCmd {
    /// Queue depth (M5 phase 1: memory + sqlite real).
    Status,
    /// Enqueue a job payload (M5 phase 1).
    Push {
        #[arg(long, help = "Job payload (JSON string, max 256KB)")]
        payload: String,
        #[arg(
            long,
            default_value_t = 3,
            help = "Max retries before dead-letter (0-25)"
        )]
        max_retries: u32,
        #[arg(long, default_value_t = 0, help = "Delay in seconds before visible")]
        delay_secs: u64,
    },
    /// Run queue worker (M5 phase 1 dev worker: reserve + ack, fail simulation via payload).
    Work {
        #[arg(
            long,
            default_value_t = 10,
            help = "Max jobs to process before exit (1-1000)"
        )]
        max_jobs: u32,
        #[arg(
            long,
            default_value_t = 60,
            help = "Visibility timeout in seconds (5-600)"
        )]
        visibility_timeout: u64,
    },
}

#[derive(Debug, Subcommand)]
enum CacheCmd {
    /// Cache stats (entries, hits/misses, bounds).
    Status,
    /// Get a key (miss = exit 0 + note, never an error).
    Get {
        #[arg(long, help = "Cache key (A-Za-z0-9 . _ - :)")]
        key: String,
    },
    /// Set a key (value max 1MB, TTL 0 = no expiry).
    Set {
        #[arg(long, help = "Cache key (A-Za-z0-9 . _ - :)")]
        key: String,
        #[arg(long, help = "Value bytes (UTF-8 string, max 1MB)")]
        value: String,
        #[arg(
            long,
            default_value_t = 0,
            help = "TTL in seconds (0 = no expiry, max 30d)"
        )]
        ttl_secs: u64,
    },
    /// Delete a key.
    Delete {
        #[arg(long, help = "Cache key")]
        key: String,
    },
    /// Clear all entries.
    Clear,
}

#[derive(Debug, Subcommand)]
enum StorageCmd {
    /// Storage stats (objects, bytes, root).
    Status,
    /// Put an object (value max 4MB).
    Put {
        #[arg(long, help = "Relative path, e.g. docs/hello.txt")]
        path: String,
        #[arg(long, help = "Value bytes (UTF-8 string, max 4MB)")]
        value: String,
    },
    /// Get an object (miss = exit 0 + note).
    Get {
        #[arg(long, help = "Relative path")]
        path: String,
    },
    /// Delete an object.
    Delete {
        #[arg(long, help = "Relative path")]
        path: String,
    },
    /// List objects by prefix (bounded, max 1000).
    List {
        #[arg(long, default_value = "", help = "Prefix filter, e.g. docs/")]
        prefix: String,
    },
}

#[derive(Debug, Subcommand)]
enum SchedulerCmd {
    /// Run scheduler dispatch (M5 phase 2: single pass, enqueue due jobs; never executes PHP).
    Run {
        #[arg(
            long,
            help = "Show what would dispatch without enqueueing (safe preview)"
        )]
        dry_run: bool,
        #[arg(
            long,
            default_value_t = 0,
            help = "Look back N seconds for missed runs (0 = current minute only, max 86400)"
        )]
        catchup_window_secs: u64,
    },
}

fn main() {
    let cli = Cli::parse();
    phpx_core::init_logging(cli.json);

    let code = match cli.command {
        None => {
            println!(
                "phpx {} (v0.1 developer preview) - try `phpx doctor` or `phpx serve --help`",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        Some(Commands::Serve { host, port }) => cmd_serve(cli.config, cli.json, host, port),
        Some(Commands::Doctor) => cmd_doctor(cli.json),
        Some(Commands::Version) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "name": "phpx", "version": env!("CARGO_PKG_VERSION"), "preview": "v0.1-developer-preview" })
                );
            } else {
                println!(
                    "phpx {} (v0.1 developer preview)",
                    env!("CARGO_PKG_VERSION")
                );
            }
            0
        }
        Some(Commands::Runtime { cmd }) => cmd_runtime(cli.config, cli.json, cmd),
        Some(Commands::Workers { cmd }) => cmd_workers(cli.config, cli.json, cmd),
        Some(Commands::Queue { cmd }) => cmd_queue(cli.config, cli.json, cmd),
        Some(Commands::Scheduler { cmd }) => cmd_scheduler(cli.config, cli.json, cmd),
        Some(Commands::Cache { cmd }) => cmd_cache(cli.config, cli.json, cmd),
        Some(Commands::Storage { cmd }) => cmd_storage(cli.config, cli.json, cmd),
        Some(Commands::Metrics) => cmd_metrics(cli.config, cli.json),
    };
    std::process::exit(code);
}

struct LoadedConfig {
    cfg: PhpxConfig,
    path: PathBuf,
}

fn resolve_config_path(arg: Option<PathBuf>) -> PathBuf {
    if let Some(p) = arg {
        return p;
    }
    let local = PathBuf::from("phpx.toml");
    if local.is_file() {
        return local;
    }
    if PathBuf::from("phpx.toml.example").is_file() {
        return PathBuf::from("phpx.toml.example");
    }
    // When invoked from a crate subdirectory (e.g. `cargo test -p phpx-cli`,
    // cwd is crates/phpx-cli), walk up to the workspace root.
    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = cwd.as_path();
        for _ in 0..4 {
            let cand = cur.join("phpx.toml.example");
            if cand.is_file() {
                return cand;
            }
            let cand2 = cur.join("phpx.toml");
            if cand2.is_file() {
                return cand2;
            }
            match cur.parent() {
                Some(parent) => cur = parent,
                None => break,
            }
        }
    }
    PathBuf::from("phpx.toml.example")
}

fn example_config_path() -> PathBuf {
    resolve_config_path(None)
}

fn load_config(arg: Option<PathBuf>) -> Result<LoadedConfig, String> {
    let path = resolve_config_path(arg);
    PhpxConfig::load(&path)
        .map(|cfg| LoadedConfig {
            cfg,
            path: path.clone(),
        })
        .map_err(|e| format!("{} (path: {})", e, path.display()))
}

fn php_first_line() -> Option<String> {
    let bin = std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string());
    Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
}

fn cmd_serve(
    config_arg: Option<PathBuf>,
    as_json: bool,
    host_ov: Option<String>,
    port_ov: Option<u16>,
) -> i32 {
    let mut loaded = match load_config(config_arg) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("config error: {msg}");
            eprintln!("hint: copy phpx.toml.example to phpx.toml and adjust, then retry with `phpx serve --config phpx.toml`");
            return 2;
        }
    };
    if let Some(h) = host_ov {
        loaded.cfg.server.host = h;
    }
    if let Some(p) = port_ov {
        loaded.cfg.server.port = p;
    }
    if let Err(e) = loaded.cfg.validate() {
        eprintln!("config error: {e}");
        eprintln!("hint: see docs/operations/configuration.md for documented defaults");
        return 2;
    }
    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "status": "starting",
                "preview": "v0.1-developer-preview",
                "config": loaded.path.display().to_string(),
                "server": { "host": loaded.cfg.server.host, "port": loaded.cfg.server.port, "tls": loaded.cfg.server.tls },
                "runtime": { "mode": loaded.cfg.runtime.mode, "php_version": loaded.cfg.runtime.php_version, "execution_mode": loaded.cfg.runtime.process.as_ref().map(|p| p.execution_mode.clone()).unwrap_or_else(|| "one-shot".to_string()), "protocol_version": phpx_runtime::PROTOCOL_VERSION },
                "health": { "liveness": "/healthz", "readiness": "/readyz" },
            })
        );
    } else {
        println!(
            "phpx serve {}:{} tls={} [v0.1 developer preview, HTTP/1.1; HTTP/2 deferred]",
            loaded.cfg.server.host, loaded.cfg.server.port, loaded.cfg.server.tls
        );
        println!(
            "config: {} | docroot: {}/{} entry={} (Ctrl-C drains gracefully)",
            loaded.path.display(),
            loaded.cfg.application.root,
            loaded.cfg.application.document_root,
            loaded.cfg.application.entrypoint
        );
        println!("health: GET /healthz (liveness), GET /readyz (readiness) | logs: structured access logs with x-request-id");
        println!("note: v0.1 is a developer preview, not a production SLA. See docs/operations/limitations.md");
    }
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot start tokio runtime: {e}");
            return 1;
        }
    };
    match rt.block_on(phpx_server::serve(loaded.cfg)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("serve failed: {e}");
            1
        }
    }
}

#[derive(Debug)]
struct Check<'a> {
    name: &'a str,
    ok: bool,
    detail: String,
}

fn cmd_doctor(as_json: bool) -> i32 {
    let mut checks: Vec<Check> = Vec::new();

    let rust_v = Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "rustc not found in PATH".to_string());
    checks.push(Check {
        name: "rust",
        ok: rust_v.starts_with("rustc "),
        detail: rust_v,
    });

    let cargo_v = Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "cargo not found in PATH".to_string());
    checks.push(Check {
        name: "cargo",
        ok: cargo_v.starts_with("cargo "),
        detail: cargo_v,
    });

    let php_bin = std::env::var("PHPX_PHP_BIN").unwrap_or_else(|_| "php".to_string());
    let php_out = Command::new(&php_bin).arg("--version").output();
    match php_out {
        Ok(o) if o.status.success() => {
            let full = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let first = full.lines().next().unwrap_or("").to_string();
            let supported = first.contains("8.3") || first.contains("8.4") || first.contains("8.5");
            checks.push(Check {
                name: "php",
                ok: true,
                detail: format!(
                    "{php_bin}: {first} (target 8.3/8.4/8.5: {})",
                    if supported {
                        "yes"
                    } else {
                        "unverified - see docs/operations/php-version-matrix.md"
                    }
                ),
            });
            let sapi = Command::new(&php_bin)
                .arg("-r")
                .arg("echo php_sapi_name();")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            let sapi_ok = sapi == "cli";
            checks.push(Check {
                name: "php-sapi",
                ok: sapi_ok,
                detail: format!(
                    "sapi={} (supported: cli one-shot; unsupported: fpm/cgi/apache handler - gateway spawns `php <entry>` directly, see docs/operations/sapi-audit.md)",
                    if sapi.is_empty() { "unknown" } else { &sapi }
                ),
            });
            let exts = Command::new(&php_bin)
                .arg("-m")
                .output()
                .map(|e| String::from_utf8_lossy(&e.stdout).to_lowercase())
                .unwrap_or_default();
            for want in [
                "json",
                "mbstring",
                "curl",
                "openssl",
                "pdo",
                "pdo_sqlite",
                "fileinfo",
                "filter",
                "dom",
                "gd",
                "intl",
            ] {
                checks.push(Check {
                    name: "php-ext",
                    ok: exts.contains(want),
                    detail: format!(
                        "{want}: {}",
                        if exts.contains(want) {
                            "found"
                        } else {
                            "missing (warning - some apps need it)"
                        }
                    ),
                });
            }
        }
        _ => {
            checks.push(Check {
                name: "php",
                ok: false,
                detail: format!("{php_bin} not found. Install PHP 8.3+ or set PHPX_PHP_BIN. See docs/operations/linux-prerequisites.md"),
            });
        }
    }

    match PhpxConfig::load(&example_config_path()) {
        Ok(_) => checks.push(Check {
            name: "config",
            ok: true,
            detail: "phpx.toml.example parses and validates (schema v1)".to_string(),
        }),
        Err(e) => checks.push(Check {
            name: "config",
            ok: false,
            detail: format!("phpx.toml.example invalid: {e}"),
        }),
    }

    // M6 gate: embedded native-link toolchain (informational; never fails core doctor).
    {
        let linked = phpx_runtime_embedded::is_build_linked();
        let abi = phpx_runtime_embedded::abi::AbiPin::pinned();
        let cfg_out = Command::new("php-config").arg("--version").output();
        match cfg_out {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let inc = Command::new("php-config")
                    .arg("--include-dir")
                    .output()
                    .map(|i| String::from_utf8_lossy(&i.stdout).trim().to_string())
                    .unwrap_or_default();
                let header_hit = ["sapi/embed/php_embed.h", "main/php.h"]
                    .iter()
                    .any(|h| std::path::Path::new(&format!("{inc}/{h}")).exists());
                checks.push(Check {
                    name: "php-embed-dev",
                    ok: true,
                    detail: format!(
                        "php-config {v}, include-dir={} (embed header {}); linked={linked}; {}",
                        if inc.is_empty() { "unknown" } else { &inc },
                        if header_hit { "found" } else { "MISSING" },
                        phpx_runtime_embedded::link::LinkProbe::probe().how_to_enable,
                    ),
                });
                let _ = abi;
            }
            _ => {
                checks.push(Check {
                    name: "php-embed-dev",
                    ok: true,
                    detail: format!(
                        "php-config not found (expected on Windows dev; embedded stays experimental-disabled, linked={linked}); Linux gate: {}",
                        phpx_runtime_embedded::link::LinkProbe::probe().how_to_enable,
                    ),
                });
            }
        }
        checks.push(Check {
            name: "php-embed-link",
            ok: true,
            detail: format!(
                "linked={linked} (opt-in native link; available=false until compat/isolation/stability gates pass; process stays default)"
            ),
        });
    }

    let os = std::env::consts::OS;
    checks.push(Check {
        name: "os",
        ok: true,
        detail: format!("{os} detected. Production target is Linux / Docker; Windows/macOS via WSL2/Docker for dev"),
    });

    let has_fail = checks.iter().any(|c| {
        !c.ok && (c.name == "php" || c.name == "rust" || c.name == "cargo" || c.name == "config")
    });

    if as_json {
        let v: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| serde_json::json!({ "name": c.name, "ok": c.ok, "detail": c.detail }))
            .collect();
        println!(
            "{}",
            serde_json::json!({ "preview": "v0.1-developer-preview", "checks": v })
        );
    } else {
        println!("phpx doctor (M4 hardening)");
        for c in &checks {
            let mark = if c.ok { "OK  " } else { "FAIL" };
            println!("  [{mark}] {} - {}", c.name, c.detail);
        }
        if has_fail {
            println!("\nAction: install missing prerequisites, then re-run `phpx doctor`.");
        } else {
            println!("\nAll core prerequisites look good for M4.");
            println!("Next: `phpx serve --config phpx.toml.example` then open http://127.0.0.1:8080/healthz and /metrics");
        }
    }
    if has_fail {
        1
    } else {
        0
    }
}

fn cmd_runtime(config_arg: Option<PathBuf>, as_json: bool, cmd: RuntimeCmd) -> i32 {
    let loaded = match load_config(config_arg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return 2;
        }
    };
    let cfg = loaded.cfg;
    let workers = cfg.runtime.process.as_ref().map(|p| p.workers).unwrap_or(4);
    let execution_mode = cfg
        .runtime
        .process
        .as_ref()
        .map(|p| p.execution_mode.clone())
        .unwrap_or_else(|| "one-shot".to_string());
    let actual_php = php_first_line().unwrap_or_else(|| "php not found in PATH".to_string());
    match cmd {
        RuntimeCmd::List => {
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "active": cfg.runtime.mode,
                        "execution_mode": execution_mode,
                        "protocol_version": phpx_runtime::PROTOCOL_VERSION,
                        "runtimes": [
                            { "name": "process", "status": if cfg.runtime.mode == "process" { "active" } else { "available" }, "note": "bounded pool (P0-B: one-shot or persistent supervised workers)" },
                            { "name": "embedded", "status": if cfg.runtime.embedded.as_ref().is_some_and(|e| e.enabled) { "experimental-enabled" } else { "experimental-disabled" }, "available": false, "linked": phpx_runtime_embedded::is_build_linked(), "note": "M6 ABI pin: php-embed not linked for execution; process fallback active", "abi": phpx_runtime_embedded::abi::AbiPin::pinned() }
                        ],
                        "php_target": cfg.runtime.php_version,
                        "php_actual": actual_php,
                    })
                );
            } else {
                println!("runtimes (config: {}):", loaded.path.display());
                println!("  execution_mode: {execution_mode} (one-shot: one fresh php per request; persistent: fixed-size supervised workers)");
                println!(
                    "  * process  - {} (default, M1 bounded pool, workers={})",
                    if cfg.runtime.mode == "process" {
                        "active"
                    } else {
                        "available"
                    },
                    workers
                );
                println!(
                    "    {}",
                    phpx_runtime_embedded::status_line(
                        cfg.runtime.embedded.as_ref().is_some_and(|e| e.enabled)
                    )
                );
                println!(
                    "php target: {} | actual: {}",
                    cfg.runtime.php_version, actual_php
                );
            }
            0
        }
        RuntimeCmd::Inspect => {
            let p = cfg.runtime.process.as_ref();
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "mode": cfg.runtime.mode,
                        "execution_mode": p.map(|x| x.execution_mode.clone()).unwrap_or_else(|| "one-shot".to_string()),
                        "protocol_version": phpx_runtime::PROTOCOL_VERSION,
                        "php_target": cfg.runtime.php_version,
                        "php_actual": actual_php,
                        "workers": p.map(|x| x.workers).unwrap_or(4),
                        "min_workers": p.map(|x| x.min_workers).unwrap_or(1),
                        "queue_bound": p.map(|x| x.effective_queue_bound()).unwrap_or(16),
                        "queue_wait_timeout_seconds": p.map(|x| x.queue_wait_timeout_seconds).unwrap_or(2),
                        "shutdown_drain_timeout_seconds": p.map(|x| x.shutdown_drain_timeout_seconds).unwrap_or(10),
                        "restart_max_retries": p.map(|x| x.restart_max_retries).unwrap_or(5),
                        "request_timeout_seconds": p.map(|x| x.request_timeout_seconds).unwrap_or(30),
                        "startup_timeout_seconds": p.map(|x| x.startup_timeout_seconds).unwrap_or(10),
                        "memory_limit": p.map(|x| x.memory_limit.clone()).unwrap_or_else(|| "256M".to_string()),
                        "docroot": format!("{}/{}", cfg.application.root, cfg.application.document_root),
                        "entrypoint": cfg.application.entrypoint,
                        "tls": cfg.server.tls,
                        "embedded": { "enabled": cfg.runtime.embedded.as_ref().is_some_and(|e| e.enabled), "available": false, "linked": phpx_runtime_embedded::is_build_linked(), "single_threaded": true, "max_concurrent_entries": 1, "abi": phpx_runtime_embedded::abi::AbiPin::pinned() },
                    })
                );
            } else {
                println!("runtime inspect ({}):", loaded.path.display());
                println!(
                    "  mode: {} | execution_mode: {} (protocol v{})",
                    cfg.runtime.mode,
                    p.map(|x| x.execution_mode.clone())
                        .unwrap_or_else(|| "one-shot".to_string()),
                    phpx_runtime::PROTOCOL_VERSION
                );
                println!(
                    "  php target: {} | actual: {}",
                    cfg.runtime.php_version, actual_php
                );
                println!(
                    "  workers: min={} max={} | queue_bound={} queue_wait={}s drain={}s",
                    p.map(|x| x.min_workers).unwrap_or(1),
                    p.map(|x| x.workers).unwrap_or(4),
                    p.map(|x| x.effective_queue_bound()).unwrap_or(16),
                    p.map(|x| x.queue_wait_timeout_seconds).unwrap_or(2),
                    p.map(|x| x.shutdown_drain_timeout_seconds).unwrap_or(10),
                );
                println!(
                    "  request_timeout={}s startup_timeout={}s memory_limit={} (advisory, unenforced in one-shot) restarts={} backoff={}ms/{}ms recycle_every={}",
                    p.map(|x| x.request_timeout_seconds).unwrap_or(30),
                    p.map(|x| x.startup_timeout_seconds).unwrap_or(10),
                    p.map(|x| x.memory_limit.clone())
                        .unwrap_or_else(|| "256M".to_string()),
                    p.map(|x| x.restart_max_retries).unwrap_or(5),
                    p.map(|x| x.restart_backoff_ms).unwrap_or(250),
                    p.map(|x| x.restart_backoff_max_ms).unwrap_or(30000),
                    p.map(|x| x.max_requests).unwrap_or(1000),
                );
                println!(
                    "  docroot: {}/{} entry={}",
                    cfg.application.root, cfg.application.document_root, cfg.application.entrypoint
                );
                println!(
                    "  tls: {} | server: {}:{}",
                    cfg.server.tls, cfg.server.host, cfg.server.port
                );
                println!(
                    "  {}",
                    phpx_runtime_embedded::status_line(
                        cfg.runtime.embedded.as_ref().is_some_and(|e| e.enabled)
                    )
                );
            }
            0
        }
    }
}

fn cmd_workers(config_arg: Option<PathBuf>, as_json: bool, cmd: WorkersCmd) -> i32 {
    let loaded = match load_config(config_arg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return 2;
        }
    };
    let cfg = loaded.cfg;
    let proc = cfg.runtime.process.clone().unwrap_or_default();
    let workers = proc.workers;
    let min_workers = proc.min_workers;
    let execution_mode = proc.execution_mode.clone();
    let queue_bound = proc.effective_queue_bound();
    let timeout_s = proc.request_timeout_seconds;
    match cmd {
        WorkersCmd::Status => {
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "pool": "bounded",
                        "execution_mode": execution_mode,
                        "protocol_version": phpx_runtime::PROTOCOL_VERSION,
                        "min_workers": min_workers,
                        "max_workers": workers,
                        "queue_bound": queue_bound,
                        "queue_wait_timeout_seconds": proc.queue_wait_timeout_seconds,
                        "execution_timeout_seconds": timeout_s,
                        "shutdown_drain_timeout_seconds": proc.shutdown_drain_timeout_seconds,
                        "restart_max_retries": proc.restart_max_retries,
                        "restart_backoff_ms": proc.restart_backoff_ms,
                        "restart_backoff_max_ms": proc.restart_backoff_max_ms,
                        "max_requests_per_slot": proc.max_requests,
                        "php_memory_limit": proc.memory_limit,
                        "memory_soft_limit": proc.memory_soft_limit,
                        "memory_hard_limit": proc.memory_hard_limit,
                        "memory_sample_interval_seconds": proc.memory_sample_interval_seconds,
                        "metrics": "GET /metrics (Prometheus text: completed/failed/timeouts/rejected/recycled/restarts/circuit/active/waiting)",
                        "lifecycle": "Failed = terminal attempt with diagnostics + restart-budget; Stopped = drained pool, no new work (SPEC 5.1 + ADR-0014)",
                        "note": "M4 hardened: crash-loop circuit breaker fast-503s; drain rejects with 503 + Retry-After: 1; overload maps to HTTP 503 + Retry-After: 1"
                    })
                );
            } else {
                println!(
                    "workers ({}): bounded pool execution_mode={} min={} max={} queue_bound={}",
                    loaded.path.display(),
                    execution_mode,
                    min_workers,
                    workers,
                    queue_bound
                );
                println!(
                    "  queue_wait={}s exec={}s drain={}s | restarts={} backoff={}ms/{}ms recycle_every={}",
                    proc.queue_wait_timeout_seconds,
                    timeout_s,
                    proc.shutdown_drain_timeout_seconds,
                    proc.restart_max_retries,
                    proc.restart_backoff_ms,
                    proc.restart_backoff_max_ms,
                    proc.max_requests,
                );
                println!(
                    "  memory: php_heap_limit={} (PHP `-d memory_limit`, heap only) | guardian soft={} hard={} sample_every={}s (OS RSS; persistent mode only)",
                    proc.memory_limit,
                    proc.memory_soft_limit,
                    proc.memory_hard_limit,
                    proc.memory_sample_interval_seconds,
                );
                println!("  overload/circuit/draining -> HTTP 503 + Retry-After: 1 (never unbounded, SPEC 5.3)");
                println!("  lifecycle: Failed = terminal attempt + diagnostics; Stopped = drained, no new work");
                println!("  metrics: GET /metrics (Prometheus) or `phpx metrics` for guidance");
            }
            0
        }
        WorkersCmd::Reload => {
            if let Err(e) = cfg.validate() {
                eprintln!("config error: {e}");
                return 2;
            }
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "reload": "ok-config-valid",
                        "action": "stateless gateway: restart `phpx serve` or `systemctl restart phpx` to apply (SIGTERM drains gracefully up to shutdown_drain_timeout_seconds); SIGHUP live-reload not supported"
                    })
                );
            } else {
                println!("reload: config {} is valid.", loaded.path.display());
                println!("M4 gateway is stateless: restart `phpx serve` or `systemctl restart phpx` to apply changes (SIGTERM drains in-flight first).");
                println!("Crash recovery: automatic (fresh process per request); Failed vs Stopped: see docs/operations/compatibility-matrix.md.");
            }
            0
        }
    }
}

fn queue_driver_from_config(
    config_arg: Option<PathBuf>,
) -> Result<(String, Option<String>, PathBuf), String> {
    let loaded = load_config(config_arg).map_err(|e| e.to_string())?;
    if let Err(e) = loaded.cfg.validate() {
        return Err(format!("{e} (config: {})", loaded.path.display()));
    }
    let (driver, database) = match &loaded.cfg.queue {
        Some(q) => (q.driver.clone(), q.database.clone()),
        None => ("sqlite".to_string(), Some("./data/queue.db".to_string())),
    };
    Ok((driver, database, loaded.path))
}

fn cmd_queue(config_arg: Option<PathBuf>, as_json: bool, cmd: QueueCmd) -> i32 {
    match cmd {
        QueueCmd::Status => {
            let (driver, database, cfg_path) = match queue_driver_from_config(config_arg) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("config error: {e}");
                    return 2;
                }
            };
            let use_db = database.clone().unwrap_or_default();
            if driver.eq_ignore_ascii_case("memory") {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "driver": "memory",
                            "depth": 0, "delayed": 0, "running": 0, "dead_letter": 0,
                            "max_depth": phpx_queue::MAX_QUEUE_DEPTH,
                            "note": "memory driver is ephemeral per process; depth is always 0 for a fresh CLI invocation",
                            "config": cfg_path.display().to_string(),
                        })
                    );
                } else {
                    println!("queue status (M5 phase 1): driver=memory (ephemeral, non-durable)");
                    println!("  depth=0 delayed=0 running=0 dead_letter=0 (fresh process, dev/testing only)");
                    println!("  config: {}", cfg_path.display());
                    println!("  tip: use driver=sqlite for durable jobs that survive restart");
                }
                return 0;
            }
            if driver.eq_ignore_ascii_case("sqlite") && !std::path::Path::new(&use_db).is_file() {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "driver": "sqlite", "database": use_db,
                            "depth": 0, "delayed": 0, "running": 0, "dead_letter": 0,
                            "note": "no db file yet; push a job first",
                            "config": cfg_path.display().to_string(),
                        })
                    );
                } else {
                    println!("queue status (M5 phase 1): driver=sqlite db={use_db}");
                    println!("  depth=0 delayed=0 running=0 dead_letter=0 (no db file yet; push a job first)");
                    println!("  config: {}", cfg_path.display());
                }
                return 0;
            }
            match phpx_queue::open_queue(&driver, database.as_deref(), phpx_queue::MAX_QUEUE_DEPTH)
            {
                Ok(q) => {
                    match q.status() {
                        Ok(st) => {
                            if as_json {
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "driver": st.driver, "database": database,
                                        "depth": st.depth, "delayed": st.delayed,
                                        "running": st.running, "dead_letter": st.dead_letter,
                                        "max_depth": st.max_depth,
                                        "config": cfg_path.display().to_string(),
                                    })
                                );
                            } else {
                                println!(
                                    "queue status (M5 phase 1): driver={} db={}",
                                    st.driver,
                                    database.unwrap_or_default()
                                );
                                println!(
                                    "  depth={} delayed={} running={} dead_letter={} (bound {})",
                                    st.depth, st.delayed, st.running, st.dead_letter, st.max_depth
                                );
                                println!("  config: {}", cfg_path.display());
                                println!("  at-least-once delivery; consumers must be idempotent (SPEC 7.1)");
                            }
                            0
                        }
                        Err(e) => {
                            eprintln!("queue status failed: {e}");
                            1
                        }
                    }
                }
                Err(e) => {
                    eprintln!("queue open failed: {e}");
                    2
                }
            }
        }
        QueueCmd::Push {
            payload,
            max_retries,
            delay_secs,
        } => {
            let (driver, database, _) = match queue_driver_from_config(config_arg) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("config error: {e}");
                    return 2;
                }
            };
            if driver.eq_ignore_ascii_case("memory") {
                eprintln!("queue push refused: driver=memory is ephemeral (jobs would vanish on exit). Set [queue] driver=sqlite for durable push.");
                return 2;
            }
            match phpx_queue::open_queue(&driver, database.as_deref(), phpx_queue::MAX_QUEUE_DEPTH)
            {
                Ok(q) => match q.enqueue(&payload, max_retries.min(25), delay_secs.min(86400)) {
                    Ok(job) => {
                        if as_json {
                            println!(
                                "{}",
                                serde_json::json!({ "id": job.id, "not_before": job.not_before, "max_retries": job.max_retries })
                            );
                        } else {
                            println!(
                                "queued {} (attempts 0/{}, visible in {}s)",
                                job.id,
                                job.max_retries,
                                delay_secs.min(86400)
                            );
                        }
                        0
                    }
                    Err(e) => {
                        eprintln!("queue push failed: {e}");
                        1
                    }
                },
                Err(e) => {
                    eprintln!("queue open failed: {e}");
                    2
                }
            }
        }
        QueueCmd::Work {
            max_jobs,
            visibility_timeout,
        } => {
            let (driver, database, _) = match queue_driver_from_config(config_arg) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("config error: {e}");
                    return 2;
                }
            };
            let max_jobs = max_jobs.clamp(1, 1000) as usize;
            let vis = visibility_timeout.clamp(5, 600);
            let q = match phpx_queue::open_queue(
                &driver,
                database.as_deref(),
                phpx_queue::MAX_QUEUE_DEPTH,
            ) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("queue open failed: {e}");
                    return 2;
                }
            };
            if driver.eq_ignore_ascii_case("memory") {
                eprintln!("note: driver=memory is ephemeral; work will find 0 jobs unless pushed in the same process (dev only).");
            }
            let mut done = 0usize;
            let mut retried = 0usize;
            let mut dead = 0usize;
            for _ in 0..max_jobs {
                let job = match q.reserve(vis) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("reserve failed: {e}");
                        return 1;
                    }
                };
                let Some(job) = job else { break };
                let should_fail = job.payload.contains("fail-job");
                if should_fail {
                    match q.fail(&job.id) {
                        Ok(phpx_queue::FailOutcome::Retrying { attempts, .. }) => {
                            retried += 1;
                            eprintln!(
                                "job {} failed (attempt {}, retrying with backoff)",
                                job.id, attempts
                            );
                        }
                        Ok(phpx_queue::FailOutcome::Dead { attempts }) => {
                            dead += 1;
                            eprintln!(
                                "job {} dead after {attempts} attempts (dead-letter)",
                                job.id
                            );
                        }
                        Err(e) => {
                            eprintln!("fail failed for {}: {e}", job.id);
                            return 1;
                        }
                    }
                } else {
                    match q.ack(&job.id) {
                        Ok(true) => done += 1,
                        Ok(false) => eprintln!("job {} already gone", job.id),
                        Err(e) => {
                            eprintln!("ack failed for {}: {e}", job.id);
                            return 1;
                        }
                    }
                }
            }
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({ "done": done, "retried": retried, "dead": dead, "max_jobs": max_jobs })
                );
            } else {
                println!("queue work done: acked={done} retried={retried} dead={dead} (max_jobs={max_jobs}, at-least-once, idempotent consumer required)");
            }
            0
        }
    }
}

fn cmd_scheduler(config_arg: Option<PathBuf>, as_json: bool, cmd: SchedulerCmd) -> i32 {
    match cmd {
        SchedulerCmd::Run {
            dry_run,
            catchup_window_secs,
        } => {
            let loaded = match load_config(config_arg) {
                Ok(c) => c,
                Err(msg) => {
                    eprintln!("config error: {msg}");
                    return 2;
                }
            };
            if let Err(e) = loaded.cfg.validate() {
                eprintln!("config error: {e} (config: {})", loaded.path.display());
                return 2;
            }
            let sched_cfg = match &loaded.cfg.scheduler {
                Some(s) => s.clone(),
                None => {
                    return scheduler_disabled(as_json, &loaded.path, "no [scheduler] section");
                }
            };
            if !sched_cfg.enabled {
                return scheduler_disabled(as_json, &loaded.path, "[scheduler] enabled=false");
            }
            if sched_cfg.schedules.is_empty() {
                return scheduler_disabled(
                    as_json,
                    &loaded.path,
                    "enabled but no [[scheduler.schedules]] configured",
                );
            }
            // Scheduler only dispatches into the queue (SPEC 7.2); it never
            // executes PHP, so the runtime backend is irrelevant here.
            let (driver, database) = match &loaded.cfg.queue {
                Some(q) => (q.driver.clone(), q.database.clone()),
                None => ("sqlite".to_string(), Some("./data/queue.db".to_string())),
            };
            if driver.eq_ignore_ascii_case("memory") && !dry_run {
                eprintln!(
                    "scheduler dispatch refused: driver=memory is ephemeral (jobs would vanish on exit). Set [queue] driver=sqlite for durable dispatch, or use --dry-run to preview."
                );
                return 2;
            }
            let history_desc: String;
            let history: Box<dyn phpx_scheduler::HistoryStore> = match &sched_cfg.history_file {
                Some(f) if !f.trim().is_empty() => {
                    match phpx_scheduler::SqliteHistory::open(std::path::Path::new(f)) {
                        Ok(h) => {
                            history_desc = format!("sqlite:{f}");
                            Box::new(h)
                        }
                        Err(e) => {
                            eprintln!("scheduler history open failed: {e}");
                            return 2;
                        }
                    }
                }
                _ => {
                    history_desc = "memory (ephemeral; dedup only within this process)".to_string();
                    Box::new(phpx_scheduler::MemoryHistory::new())
                }
            };
            let queue = match phpx_queue::open_queue(
                &driver,
                database.as_deref(),
                phpx_queue::MAX_QUEUE_DEPTH,
            ) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("queue open failed: {e}");
                    return 2;
                }
            };
            let mut defs = Vec::with_capacity(sched_cfg.schedules.len());
            for s in &sched_cfg.schedules {
                match phpx_scheduler::ScheduleDef::new(
                    &s.id,
                    &s.cron,
                    &s.timezone,
                    &s.payload,
                    s.enabled,
                    &s.missed_policy,
                    s.max_retries,
                ) {
                    Ok(d) => defs.push(d),
                    Err(e) => {
                        eprintln!("invalid schedule: {e} (config: {})", loaded.path.display());
                        return 2;
                    }
                }
            }
            let scheduler = match phpx_scheduler::Scheduler::new(defs, queue, history) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("scheduler setup failed: {e}");
                    return 2;
                }
            };
            let catchup = catchup_window_secs.min(86_400);
            let now = phpx_queue::now_unix();
            // Align to minute boundaries: dues are minute slots, and
            // `next_after` searches strictly after the cursor.
            let end_min = phpx_scheduler::ScheduleDef::due_minute(now);
            // Always include the current minute slot: the window is
            // exclusive on the left, so look back at least 60s.
            let start = end_min.saturating_sub(catchup.max(60));
            let dispatch_missed = catchup > 0;
            if dry_run {
                match scheduler.plan_window(start, now, dispatch_missed) {
                    Ok(plan) => {
                        print_scheduler_plan(
                            as_json,
                            &loaded.path,
                            &history_desc,
                            true,
                            &plan,
                            catchup,
                        );
                        0
                    }
                    Err(e) => {
                        eprintln!("scheduler plan failed: {e}");
                        1
                    }
                }
            } else {
                match scheduler.tick_window(start, now, dispatch_missed) {
                    Ok(outcome) => {
                        print_scheduler_outcome(
                            as_json,
                            &loaded.path,
                            &history_desc,
                            false,
                            &outcome,
                            catchup,
                        );
                        0
                    }
                    Err(e) => {
                        eprintln!("scheduler dispatch failed: {e}");
                        1
                    }
                }
            }
        }
    }
}

fn scheduler_disabled(as_json: bool, path: &std::path::Path, reason: &str) -> i32 {
    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "status": "disabled",
                "surface": "scheduler run",
                "reason": reason,
                "dispatched": [],
                "config": path.display().to_string(),
            })
        );
    } else {
        println!("scheduler run (M5 phase 2): disabled ({reason}).");
        println!("  config: {}", path.display());
        println!("  hint: set [scheduler] enabled=true and add [[scheduler.schedules]] (see docs/operations/scheduler.md).");
    }
    0
}

fn print_scheduler_plan(
    as_json: bool,
    path: &std::path::Path,
    history: &str,
    dry_run: bool,
    plan: &phpx_scheduler::PlanOutcome,
    catchup: u64,
) {
    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "surface": "scheduler run",
                "dry_run": dry_run,
                "catchup_window_secs": catchup,
                "history": history,
                "to_dispatch": plan.to_dispatch.iter().map(|d| serde_json::json!({
                    "schedule": d.schedule_id, "due_at": d.due_at,
                })).collect::<Vec<_>>(),
                "skipped": {
                    "duplicate": plan.skipped_duplicate,
                    "missed": plan.skipped_missed,
                    "disabled": plan.skipped_disabled,
                },
                "config": path.display().to_string(),
            })
        );
        return;
    }
    println!("scheduler run (M5 phase 2): dry-run plan, history={history}");
    if plan.to_dispatch.is_empty() {
        println!(
            "  nothing due (duplicate={} missed={} disabled={})",
            plan.skipped_duplicate, plan.skipped_missed, plan.skipped_disabled
        );
    } else {
        for d in &plan.to_dispatch {
            println!(
                "  would dispatch: {} due_at={} (unix)",
                d.schedule_id, d.due_at
            );
        }
        println!(
            "  skipped: duplicate={} missed={} disabled={}",
            plan.skipped_duplicate, plan.skipped_missed, plan.skipped_disabled
        );
    }
    println!("  catchup_window={catchup}s | config: {}", path.display());
    println!("  note: scheduler only dispatches (enqueue); it never executes PHP (SPEC 7.2).");
}

fn print_scheduler_outcome(
    as_json: bool,
    path: &std::path::Path,
    history: &str,
    dry_run: bool,
    outcome: &phpx_scheduler::TickOutcome,
    catchup: u64,
) {
    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "surface": "scheduler run",
                "dry_run": dry_run,
                "catchup_window_secs": catchup,
                "history": history,
                "dispatched": outcome.dispatched.iter().map(|d| serde_json::json!({
                    "schedule": d.schedule_id, "due_at": d.due_at, "job_id": d.job_id,
                })).collect::<Vec<_>>(),
                "skipped": {
                    "duplicate": outcome.skipped_duplicate,
                    "missed": outcome.skipped_missed,
                    "disabled": outcome.skipped_disabled,
                },
                "config": path.display().to_string(),
            })
        );
        return;
    }
    println!("scheduler run (M5 phase 2): single pass, history={history}");
    if outcome.dispatched.is_empty() {
        println!(
            "  dispatched 0 jobs (duplicate={} missed={} disabled={})",
            outcome.skipped_duplicate, outcome.skipped_missed, outcome.skipped_disabled
        );
    } else {
        for d in &outcome.dispatched {
            println!(
                "  dispatched: {} due_at={} job={}",
                d.schedule_id, d.due_at, d.job_id
            );
        }
        println!(
            "  skipped: duplicate={} missed={} disabled={}",
            outcome.skipped_duplicate, outcome.skipped_missed, outcome.skipped_disabled
        );
    }
    println!("  catchup_window={catchup}s | config: {}", path.display());
    println!("  note: scheduler only dispatches (enqueue); it never executes PHP (SPEC 7.2).");
}

type CacheCfg = (String, Option<String>, usize, u64, PathBuf);
type StorageCfg = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    PathBuf,
);

fn cache_from_config(config_arg: Option<PathBuf>) -> Result<CacheCfg, String> {
    let loaded = load_config(config_arg).map_err(|e| e.to_string())?;
    if let Err(e) = loaded.cfg.validate() {
        return Err(format!("{e} (config: {})", loaded.path.display()));
    }
    let (driver, dir, max_entries, max_bytes) = match &loaded.cfg.cache {
        Some(c) => {
            let entries = c.max_entries.unwrap_or(1000).clamp(1, 100_000);
            let bytes = match &c.max_memory {
                Some(s) => phpx_config::parse_size(s)
                    .map_err(|e| format!("{e} (config: {})", loaded.path.display()))?,
                None => 128 * 1024 * 1024,
            };
            (c.driver.clone(), c.dir.clone(), entries, bytes)
        }
        None => ("memory".to_string(), None, 1000, 128 * 1024 * 1024),
    };
    Ok((driver, dir, max_entries, max_bytes, loaded.path))
}

fn storage_from_config(config_arg: Option<PathBuf>) -> Result<StorageCfg, String> {
    let loaded = load_config(config_arg).map_err(|e| e.to_string())?;
    if let Err(e) = loaded.cfg.validate() {
        return Err(format!("{e} (config: {})", loaded.path.display()));
    }
    let (driver, root, endpoint, bucket, region) = match &loaded.cfg.storage {
        Some(s) => (
            s.driver.clone(),
            s.root.clone(),
            s.endpoint.clone(),
            s.bucket.clone(),
            s.region.clone(),
        ),
        None => (
            "local".to_string(),
            Some("./data/storage".to_string()),
            None,
            None,
            None,
        ),
    };
    Ok((driver, root, endpoint, bucket, region, loaded.path))
}

fn cmd_cache(config_arg: Option<PathBuf>, as_json: bool, cmd: CacheCmd) -> i32 {
    let (driver, dir, max_entries, max_bytes, cfg_path) = match cache_from_config(config_arg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("config error: {e}");
            return 2;
        }
    };
    let cache = match phpx_cache::open_cache(&driver, dir.as_deref(), max_entries, max_bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cache open failed: {e}");
            return 2;
        }
    };
    match cmd {
        CacheCmd::Status => match cache.stats() {
            Ok(st) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "driver": st.driver, "entries": st.entries,
                            "hits": st.hits, "misses": st.misses,
                            "max_entries": st.max_entries, "max_bytes": st.max_bytes,
                            "config": cfg_path.display().to_string(),
                        })
                    );
                } else {
                    println!(
                        "cache status (M5 phase 3): driver={} entries={} hits={} misses={} (bound {} entries / {} bytes)",
                        st.driver, st.entries, st.hits, st.misses, st.max_entries, st.max_bytes
                    );
                    println!("  config: {}", cfg_path.display());
                }
                0
            }
            Err(e) => {
                eprintln!("cache status failed: {e}");
                1
            }
        },
        CacheCmd::Get { key } => match cache.get(&key) {
            Ok(Some(v)) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({ "key": key, "hit": true, "value": String::from_utf8_lossy(&v) })
                    );
                } else {
                    println!("cache hit: {key} = {}", String::from_utf8_lossy(&v));
                }
                0
            }
            Ok(None) => {
                if as_json {
                    println!("{}", serde_json::json!({ "key": key, "hit": false }));
                } else {
                    println!("cache miss: {key} (no entry or expired)");
                }
                0
            }
            Err(e) => {
                eprintln!("cache get failed: {e}");
                2
            }
        },
        CacheCmd::Set {
            key,
            value,
            ttl_secs,
        } => match cache.set(&key, value.as_bytes(), ttl_secs.min(86400 * 30)) {
            Ok(()) => {
                if as_json {
                    println!("{}", serde_json::json!({ "key": key, "stored": true }));
                } else {
                    println!(
                        "cache stored: {key} ({} bytes, ttl={}s)",
                        value.len(),
                        ttl_secs
                    );
                }
                0
            }
            Err(e) => {
                eprintln!("cache set failed: {e}");
                2
            }
        },
        CacheCmd::Delete { key } => match cache.delete(&key) {
            Ok(true) => {
                if as_json {
                    println!("{}", serde_json::json!({ "key": key, "deleted": true }));
                } else {
                    println!("cache deleted: {key}");
                }
                0
            }
            Ok(false) => {
                if as_json {
                    println!("{}", serde_json::json!({ "key": key, "deleted": false }));
                } else {
                    println!("cache miss on delete: {key} (nothing to remove)");
                }
                0
            }
            Err(e) => {
                eprintln!("cache delete failed: {e}");
                2
            }
        },
        CacheCmd::Clear => match cache.clear() {
            Ok(n) => {
                if as_json {
                    println!("{}", serde_json::json!({ "cleared": n }));
                } else {
                    println!("cache cleared: {n} entries removed");
                }
                0
            }
            Err(e) => {
                eprintln!("cache clear failed: {e}");
                1
            }
        },
    }
}

fn cmd_storage(config_arg: Option<PathBuf>, as_json: bool, cmd: StorageCmd) -> i32 {
    let (driver, root, endpoint, bucket, region, cfg_path) = match storage_from_config(config_arg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("config error: {e}");
            return 2;
        }
    };
    let store = match phpx_storage::open_storage(
        &driver,
        root.as_deref(),
        endpoint.as_deref(),
        bucket.as_deref(),
        region.as_deref(),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage open failed: {e}");
            return 2;
        }
    };
    match cmd {
        StorageCmd::Status => match store.status() {
            Ok(st) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "driver": st.driver, "objects": st.objects, "bytes": st.bytes,
                            "root": st.root, "config": cfg_path.display().to_string(),
                        })
                    );
                } else {
                    println!(
                        "storage status (M5 phase 3): driver={} objects={} bytes={} root={}",
                        st.driver, st.objects, st.bytes, st.root
                    );
                    println!("  config: {}", cfg_path.display());
                }
                0
            }
            Err(e) => {
                eprintln!("storage status failed: {e}");
                1
            }
        },
        StorageCmd::Put { path, value } => match store.put(&path, value.as_bytes()) {
            Ok(()) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({ "path": path, "stored": true, "bytes": value.len() })
                    );
                } else {
                    println!("storage stored: {path} ({} bytes)", value.len());
                }
                0
            }
            Err(e) => {
                eprintln!("storage put failed: {e}");
                2
            }
        },
        StorageCmd::Get { path } => match store.get(&path) {
            Ok(Some(v)) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({ "path": path, "hit": true, "value": String::from_utf8_lossy(&v) })
                    );
                } else {
                    println!("storage hit: {path} = {}", String::from_utf8_lossy(&v));
                }
                0
            }
            Ok(None) => {
                if as_json {
                    println!("{}", serde_json::json!({ "path": path, "hit": false }));
                } else {
                    println!("storage miss: {path} (no such object)");
                }
                0
            }
            Err(e) => {
                eprintln!("storage get failed: {e}");
                2
            }
        },
        StorageCmd::Delete { path } => match store.delete(&path) {
            Ok(true) => {
                if as_json {
                    println!("{}", serde_json::json!({ "path": path, "deleted": true }));
                } else {
                    println!("storage deleted: {path}");
                }
                0
            }
            Ok(false) => {
                if as_json {
                    println!("{}", serde_json::json!({ "path": path, "deleted": false }));
                } else {
                    println!("storage miss on delete: {path}");
                }
                0
            }
            Err(e) => {
                eprintln!("storage delete failed: {e}");
                2
            }
        },
        StorageCmd::List { prefix } => match store.list(&prefix) {
            Ok(items) => {
                if as_json {
                    println!(
                        "{}",
                        serde_json::json!({ "prefix": prefix, "objects": items })
                    );
                } else {
                    println!(
                        "storage list prefix={prefix:?} ({} objects, max 1000):",
                        items.len()
                    );
                    for k in &items {
                        println!("  {k}");
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("storage list failed: {e}");
                2
            }
        },
    }
}

fn cmd_metrics(config_arg: Option<PathBuf>, as_json: bool) -> i32 {
    let note = "M4 metrics: structured access logs (request_id, method, path, status, duration_ms, overload/circuit flags) + GET /healthz + GET /readyz + GET /metrics (Prometheus text).";
    if as_json {
        let cfg_info = load_config(config_arg).ok().map(|l| {
            serde_json::json!({ "config": l.path.display().to_string(), "observability": l.cfg.observability })
        });
        println!(
            "{}",
            serde_json::json!({ "status": "dev-preview", "note": note, "config": cfg_info })
        );
    } else {
        println!("metrics (M4 hardening):");
        println!("  - {note}");
        println!(
            "  - tip: RUST_LOG=info phpx serve  # access logs to stdout; use --json for JSON logs"
        );
        println!("  - live: curl http://127.0.0.1:8080/metrics  # needs `phpx serve` running");
        println!("  - keys: phpx_execution_mode_info + phpx_pool_completed/failed/timeouts/rejected_overload/recycled/restarts/circuit_breaks/active/waiting");
    }
    0
}
