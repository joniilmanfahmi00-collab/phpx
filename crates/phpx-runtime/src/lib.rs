use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

/// Versioned runtime protocol. Bump on breaking envelope changes.
pub const PROTOCOL_VERSION: u32 = 1;
/// Max framed message: 16 MiB (matches default request_body_limit).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Explicit execution-mode identifiers.
/// - `one-shot`: one fresh `php` process per request (M1, implemented).
/// - `persistent`: long-lived supervised PHP workers (P0-B, implemented as
///   `PersistentPool`: fixed-size pool, handshake-gated, per-worker identity).
pub const EXECUTION_MODE_ONE_SHOT: &str = "one-shot";
pub const EXECUTION_MODE_PERSISTENT: &str = "persistent";

/// Selected execution mode. Both variants are runnable; the gateway selects
/// the pool (`BoundedPool` vs `PersistentPool`) from configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    OneShot,
    Persistent,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionMode::OneShot => EXECUTION_MODE_ONE_SHOT,
            ExecutionMode::Persistent => EXECUTION_MODE_PERSISTENT,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "one-shot" | "oneshot" => Some(ExecutionMode::OneShot),
            "persistent" => Some(ExecutionMode::Persistent),
            _ => None,
        }
    }
}

/// P0-A: persistent-worker identity contract (P0-B will populate it).
/// `worker_id` is stable across restarts of the same slot; `incarnation`
/// changes every time the OS process is (re)spawned; `pid` is the live
/// process handle while the worker is supervised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerIdentity {
    pub worker_id: u64,
    pub incarnation: u64,
    pub pid: Option<u32>,
}

impl WorkerIdentity {
    pub fn new(worker_id: u64, incarnation: u64, pid: Option<u32>) -> Self {
        Self {
            worker_id,
            incarnation,
            pid,
        }
    }
}

/// Versioned request envelope (SPEC 3.3, M1 prototype).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub query: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Vec<u8>,
    pub app_root: String,
    pub entrypoint: String,
    /// Milliseconds until deadline. 0 means no deadline set by caller.
    #[serde(default)]
    pub deadline_ms: u64,
    #[serde(default = "default_env_profile")]
    pub env_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpResponse {
    pub request_id: String,
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Vec<u8>,
    pub duration_ms: u64,
    pub outcome: WorkerOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcome {
    Ok,
    Timeout,
    Crashed { exit_code: Option<i32> },
    ProtocolError { detail: String },
}

fn default_env_profile() -> String {
    "default".to_string()
}

impl PhpRequest {
    pub fn minimal(request_id: &str, app_root: &str, entrypoint: &str) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
            query: String::new(),
            headers: HashMap::new(),
            body: Vec::new(),
            app_root: app_root.to_string(),
            entrypoint: entrypoint.to_string(),
            deadline_ms: 0,
            env_profile: default_env_profile(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(PhpxError::Config(format!(
                "unsupported protocol_version {} (expected {})",
                self.protocol_version, PROTOCOL_VERSION
            )));
        }
        if self.request_id.trim().is_empty() {
            return Err(PhpxError::Config(
                "request_id must not be empty".to_string(),
            ));
        }
        if self.body.len() > MAX_FRAME_BYTES {
            return Err(PhpxError::Config(
                "request body exceeds MAX_FRAME_BYTES".to_string(),
            ));
        }
        Ok(())
    }
}

/// Length-prefixed framing: 4-byte big-endian length + JSON bytes.
/// Shared by stdio pipes prototype and future Unix socket transport (M1).
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(value)
        .map_err(|e| PhpxError::Runtime(format!("frame encode failed: {e}")))?;
    if json.len() > MAX_FRAME_BYTES {
        return Err(PhpxError::Config(
            "frame exceeds MAX_FRAME_BYTES".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Read exactly one frame. `reader` must be blocking.
pub fn decode_frame<T: for<'de> Deserialize<'de>, R: Read>(reader: &mut R) -> Result<T> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .map_err(|e| PhpxError::Runtime(format!("frame header read failed: {e}")))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(PhpxError::Config(
            "frame header exceeds MAX_FRAME_BYTES".to_string(),
        ));
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|e| PhpxError::Runtime(format!("frame body read failed: {e}")))?;
    serde_json::from_slice(&buf)
        .map_err(|e| PhpxError::Runtime(format!("frame decode failed: {e}")))
}

/// Write one frame.
pub fn write_frame<T: Serialize, W: Write>(writer: &mut W, value: &T) -> Result<()> {
    let bytes = encode_frame(value)?;
    writer
        .write_all(&bytes)
        .map_err(|e| PhpxError::Runtime(format!("frame write failed: {e}")))?;
    writer
        .flush()
        .map_err(|e| PhpxError::Runtime(format!("frame flush failed: {e}")))?;
    Ok(())
}

/// Common runtime contract (SPEC 4.1, sync M1 shape; async orchestration lands in M2).
pub trait RuntimeBackend {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Vec<String>;
    fn execute(&self, req: &PhpRequest) -> Result<PhpResponse>;
    fn health(&self) -> String {
        "ok".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn envelope_validates_version() {
        let mut req = PhpRequest::minimal("r1", ".", "index.php");
        req.validate().unwrap();
        req.protocol_version = 999;
        assert!(req.validate().is_err());
    }

    #[test]
    fn frame_roundtrip() {
        let req = PhpRequest::minimal("req-123", ".", "index.php");
        let bytes = encode_frame(&req).unwrap();
        assert!(bytes.len() > 4);
        let mut cursor = Cursor::new(bytes);
        let back: PhpRequest = decode_frame(&mut cursor).unwrap();
        assert_eq!(back.request_id, "req-123");
    }

    #[test]
    fn execution_mode_parses_explicitly() {
        assert_eq!(
            ExecutionMode::parse("one-shot"),
            Some(ExecutionMode::OneShot)
        );
        assert_eq!(
            ExecutionMode::parse("persistent"),
            Some(ExecutionMode::Persistent)
        );
        assert_eq!(ExecutionMode::parse("bogus"), None);
        assert_eq!(ExecutionMode::OneShot.as_str(), EXECUTION_MODE_ONE_SHOT);
    }

    #[test]
    fn frame_rejects_oversize_header() {
        let bad = u32::MAX.to_be_bytes().to_vec();
        let mut cursor = Cursor::new(bad);
        let res: Result<PhpRequest> = decode_frame(&mut cursor);
        assert!(res.is_err());
    }
}
