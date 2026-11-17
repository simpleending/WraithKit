//! WraithKit kernel: sealed effects, approvals bound to data history, and a hash-chained ledger.

pub mod app;
pub mod bridge;
pub mod cli;
pub mod ledger;
pub mod model;
pub mod server;
pub mod session;
pub mod taint;
#[cfg(test)]
pub(crate) mod testkit;
pub mod verify;

pub use app::{App, Demo, EvidenceSink, Kind, Preview, ToolSpec};
pub use ledger::Mode;
pub use taint::Clean;

use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

pub const MAX_TURNS: usize = 16;
pub const MAX_TOOL_CALLS: usize = 40;
pub const TOOL_RESULT_CHARS: usize = 8_000;
pub const MODEL_TIMEOUT_SECS: u64 = 90;

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Key-sorted JSON. Keys are sorted only because serde_json's `preserve_order` feature is off
/// (test: `canonical_sorts_keys`).
pub fn canonical(v: &impl Serialize) -> String {
    // Every caller passes plain serde structs or `Value`, which cannot fail to serialize.
    serde_json::to_value(v)
        .expect("plain serde types always serialize")
        .to_string()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A fresh, unique path under the system temp folder for an app's demo files. The caller creates it.
pub fn demo_dir(app_name: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "{app_name}-demo-{}-{}-{}",
        now_ms(),
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

/// Parse model-supplied tool arguments into a typed struct. Put `#[serde(deny_unknown_fields)]` on `T`.
pub fn args<T: DeserializeOwned>(v: &serde_json::Value) -> Result<T, String> {
    T::deserialize(v).map_err(|e| format!("invalid arguments: {e}"))
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::{Path, PathBuf};

    /// A unique folder under the system temp dir, removed on drop.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!(
                "wraithkit-{tag}-{}-{}-{n}",
                std::process::id(),
                super::now_ms()
            ));
            std::fs::create_dir_all(&p).expect("create temp dir");
            TempDir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_sorts_keys() {
        let v = serde_json::json!({"b": 1, "a": {"d": 2, "c": 3}});
        assert_eq!(canonical(&v), r#"{"a":{"c":3,"d":2},"b":1}"#);
    }

    #[test]
    fn sha_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn demo_dir_is_unique() {
        let (a, b) = (demo_dir("x"), demo_dir("x"));
        assert_ne!(a, b);
        assert!(a.starts_with(std::env::temp_dir()));
        assert!(
            a.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("x-demo-")
        );
    }

    #[test]
    fn args_rejects_unknown_fields() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct A {
            x: i64,
        }
        assert!(args::<A>(&serde_json::json!({"x": 1})).is_ok());
        assert!(args::<A>(&serde_json::json!({"x": 1, "y": 2})).is_err());
    }
}
