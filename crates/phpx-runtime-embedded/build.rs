//! M6 Point 2: opt-in native php-embed link (Linux NTS only, never default).
//!
//! Never fails the build. Links libphp only when BOTH hold:
//!   - TARGET contains linux, AND
//!   - env PHPX_EMBED_LINK=1.
//! Otherwise stays stub so Windows/ZTS dev boxes stay green.
//! When linked, emits cfg(phpx_embed_linked) for diagnostics;
//! availability stays false in v0.1 until compat/isolation/stability gates pass.
use std::env;
use std::process::Command;
fn warn(msg: &str) {
    println!("cargo:warning=phpx-embedded: {msg}");
}
fn main() {
    println!("cargo:rerun-if-env-changed=PHPX_EMBED_LINK");
    println!("cargo:rerun-if-env-changed=PHP_CONFIG");
    println!("cargo:rustc-check-cfg=cfg(phpx_embed_linked)");
    let want = env::var("PHPX_EMBED_LINK").unwrap_or_default();
    if want != "1" {
        warn("native link disabled (default stub). Set PHPX_EMBED_LINK=1 on Linux NTS to try.");
        return;
    }
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("linux") {
        warn(&format!(
            "link requested but TARGET={target} is not linux; staying stub"
        ));
        return;
    }
    let php_config = env::var("PHP_CONFIG").unwrap_or_else(|_| "php-config".to_string());
    let version = Command::new(&php_config)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if version.is_empty() {
        warn(&format!("{php_config} --version failed; staying stub (install php-dev, e.g. sudo apt-get install -y php8.3-dev libphp8.3-embed)"));
        return;
    }
    let supported =
        version.starts_with("8.3") || version.starts_with("8.4") || version.starts_with("8.5");
    if !supported {
        warn(&format!(
            "php-config version {version} not in 8.3/8.4/8.5; staying stub"
        ));
        return;
    }
    let mut search_dirs: Vec<String> = Vec::new();
    let mut libs: Vec<String> = Vec::new();
    for flag in ["--ld-flags", "--libs"] {
        if let Ok(out) = Command::new(&php_config).arg(flag).output() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            for tok in s.split_whitespace() {
                if let Some(dir) = tok.strip_prefix("-L") {
                    if !dir.is_empty() && !search_dirs.contains(&dir.to_string()) {
                        search_dirs.push(dir.to_string());
                    }
                } else if let Some(lib) = tok.strip_prefix("-l") {
                    if !lib.is_empty() && !libs.contains(&lib.to_string()) {
                        libs.push(lib.to_string());
                    }
                }
            }
        }
    }
    // Debian php-config --libs lists dependencies but NOT libphp itself,
    // so probe for the embed library file and link it explicitly.
    // (M6 finding 2026-11-19: without this the link line has no -lphp at all.)
    let parts: Vec<&str> = version.split(".").collect();
    let mm = if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        version.clone()
    };
    let mm_nospace = if parts.len() >= 2 {
        format!("{}{}", parts[0], parts[1])
    } else {
        version.clone()
    };
    let candidates = vec![
        format!("php{mm}"),
        format!("php{mm_nospace}"),
        "php".to_string(),
    ];
    let mut probe_dirs = search_dirs.clone();
    for d in [
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib",
        "/lib/x86_64-linux-gnu",
        "/lib",
    ] {
        if !probe_dirs.contains(&d.to_string()) {
            probe_dirs.push(d.to_string());
        }
    }
    let mut chosen: Option<String> = None;
    for c in &candidates {
        let f = format!("lib{c}.so");
        if probe_dirs
            .iter()
            .any(|d| std::path::Path::new(&format!("{d}/{f}")).exists())
        {
            chosen = Some(c.clone());
            break;
        }
    }
    let chosen = match chosen {
        Some(c) => c,
        None => {
            warn(&format!("libphp .so not found in {probe_dirs:?} (tried {candidates:?}); staying stub (install libphpembed, e.g. sudo apt-get install -y libphp8.3-embed)"));
            return;
        }
    };
    if !libs.contains(&chosen) {
        libs.insert(0, chosen.clone());
    }
    for d in &search_dirs {
        println!("cargo:rustc-link-search=native={d}");
    }
    for l in &libs {
        println!("cargo:rustc-link-lib={l}");
    }
    println!("cargo:rustc-cfg=phpx_embed_linked");
    warn(&format!("linked php-embed (php-config {version}, libs={libs:?}); runtime stays experimental, available=false until M6 gates pass"));
}
