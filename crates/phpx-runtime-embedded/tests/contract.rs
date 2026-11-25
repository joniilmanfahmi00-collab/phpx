use phpx_runtime::PhpRequest;
use phpx_runtime_embedded as emb;

#[test]
fn envelope_roundtrip_shared_contract() {
    let r = PhpRequest::minimal("m6-1", "app", "index.php");
    let bytes = phpx_runtime::encode_frame(&r).expect("encode ok");
    let back: PhpRequest = {
        let mut s: &[u8] = &bytes;
        phpx_runtime::decode_frame(&mut s).expect("decode ok")
    };
    assert_eq!(back.request_id, "m6-1");
}

#[test]
fn capabilities_report_contract() {
    let off = emb::EmbeddedCapabilities::report(false);
    assert!(!off.available);
    assert!(off.single_threaded);
    assert_eq!(off.max_concurrent_entries, 1);
    let on = emb::EmbeddedCapabilities::report(true);
    assert!(on.enabled);
    assert!(!on.available);
}

#[test]
fn single_entry_contract() {
    let d = emb::EmbeddedDomain::new();
    let _g = d.try_enter().expect("enter ok");
    assert!(d.try_enter().is_err());
}

#[test]
fn stub_never_executes_php() {
    let r = PhpRequest::minimal("m6-2", "app", "index.php");
    let err = emb::execute_stub(&r).unwrap_err().to_string();
    assert!(err.contains("php-embed not linked"));
}

#[cfg(not(phpx_embed_linked))]
#[test]
fn live_path_fail_closed_without_link() {
    let e = emb::live::execute_once("return 1;")
        .unwrap_err()
        .to_string();
    assert!(e.contains("unavailable"), "{e}");
}

#[test]
fn abi_pin_contract() {
    let pin = emb::abi::AbiPin::pinned();
    assert_eq!(pin.supported_php_versions, vec!["8.3", "8.4", "8.5"]);
    assert_eq!(pin.sapi, "embed");
    assert!(pin.nts_only);
    assert!(emb::abi::AbiPin::supports_version("8.4.1"));
    assert!(emb::abi::AbiPin::check("8.4.1", false, false).is_ok());
    assert!(emb::abi::AbiPin::check("8.3.30", true, false).is_err());
}

#[test]
fn link_probe_contract() {
    let p = emb::link::LinkProbe::probe();
    assert!(!p.found);
    assert_eq!(p.tried, vec!["php-config --version", "php --version"]);
    assert!(p.how_to_enable.contains("Linux only"));
    assert!(emb::link::LinkProbe::build_plan().contains("execute_once"));
}

#[test]
fn bridge_contract() {
    let r = PhpRequest::minimal("m6-b", "app", "index.php");
    assert!(emb::bridge::call_stub("eval", &r).is_err());
    let e = emb::bridge::call_stub("execute_once", &r)
        .unwrap_err()
        .to_string();
    assert!(e.contains("unlinked"));
}

#[test]
fn ownership_contract() {
    assert_eq!(emb::ownership::rules().len(), 5);
    let mut s = emb::ownership::RequestScope::open();
    s.track(7);
    assert_eq!(s.finish(), 7);
}

#[test]
fn nts_zts_matrix_contract() {
    for v in ["8.3", "8.3.30", "8.4.1", "8.5.0"] {
        assert!(emb::abi::AbiPin::check(v, false, false).is_ok());
        assert!(emb::abi::AbiPin::check(v, true, false).is_err());
        assert!(emb::abi::AbiPin::check(v, false, true).is_err());
    }
    for v in ["8.2.0", "7.4.33"] {
        assert!(emb::abi::AbiPin::check(v, false, false).is_err());
    }
}

#[test]
fn compat_subset_contract() {
    let r = PhpRequest::minimal("m6-c", "app", "index.php");
    let ids: Vec<String> = emb::compat::subsets()
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert!(ids.contains(&"laravel-minimal".to_string()));
    assert!(ids.contains(&"wordpress-minimal".to_string()));
    let e = emb::compat::run_subset_stub("wordpress-minimal", &r)
        .unwrap_err()
        .to_string();
    assert!(e.contains("deferred"));
    assert!(emb::compat::run_subset_stub("drupal", &r).is_err());
}

#[test]
fn containment_contract() {
    let d = emb::EmbeddedDomain::new();
    let r = PhpRequest::minimal("m6-k", "app", "index.php");
    let e = emb::containment::execute_guarded(&d, &r)
        .unwrap_err()
        .to_string();
    assert!(e.contains("gateway stays up"));
    assert_eq!(
        emb::containment::ResourceLimits::standard().max_frame_bytes,
        phpx_runtime::MAX_FRAME_BYTES
    );
}

#[test]
fn no_arbitrary_concurrent_entry() {
    use std::sync::Arc;
    let d = Arc::new(emb::EmbeddedDomain::new());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let dd = d.clone();
        handles.push(std::thread::spawn(move || match dd.try_enter() {
            Ok(_g) => {
                std::thread::sleep(std::time::Duration::from_millis(200));
                true
            }
            Err(_) => false,
        }));
    }
    let wins = handles
        .into_iter()
        .filter_map(|h| h.join().ok())
        .filter(|ok| *ok)
        .count();
    assert_eq!(wins, 1);
}

#[test]
fn extension_matrix_contract() {
    assert_eq!(emb::ext::REQUIRED_EXTENSIONS.len(), 11);
    let r = emb::ext::ExtReport::probe();
    assert!(
        r.present.contains(&"json".to_string()),
        "live probe must parse the local php: {}",
        r.note
    );
    for ext in emb::ext::REQUIRED_EXTENSIONS {
        assert!(
            r.present.contains(&ext.to_string()) || r.missing.contains(&ext.to_string()),
            "{ext} must be classified present or missing"
        );
    }
}

#[test]
fn isolation_scope_contract() {
    let mut s = emb::isolation::IsolationScope::open("m6-iso");
    s.track(32);
    s.release(32);
    let receipt = s.seal().expect("clean seal");
    assert!(receipt.clean);
    let mut leaky = emb::isolation::IsolationScope::open("m6-leak");
    leaky.track(8);
    assert!(leaky.seal().is_err());
}

#[test]
fn engine_lifecycle_contract() {
    use emb::engine::{EngineEvent, EnginePhase};
    assert_eq!(
        EnginePhase::Cold.transition(EngineEvent::InitStart),
        Some(EnginePhase::Initializing)
    );
    assert_eq!(
        EnginePhase::Ready.transition(EngineEvent::Enter),
        Some(EnginePhase::Serving)
    );
    assert!(EnginePhase::Ready.can_enter());
    assert!(!EnginePhase::Serving.can_enter());
    assert_eq!(EnginePhase::Cold.transition(EngineEvent::Enter), None);
}

#[test]
fn promotion_gates_assess_fail_closed() {
    let gates = emb::gates::assess();
    let ids: Vec<&str> = gates.iter().map(|g| g.id).collect();
    assert_eq!(
        ids,
        vec![
            "abi_pin",
            "native_link",
            "execution_domain",
            "ownership",
            "containment",
            "extensions",
            "isolation",
            "compat_parity",
            "stability"
        ]
    );
    for g in &gates {
        assert!(!g.evidence.is_empty(), "{} needs evidence", g.id);
    }
    assert!(
        !emb::gates::available(),
        "promotion stays fail-closed until every gate passes"
    );
}
