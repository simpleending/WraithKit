//! Runs the shipped deterministic command, never a model or remote workflow.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Bundle(PathBuf);
impl Bundle {
    fn new() -> Self {
        let root = wraithkit::demo_dir("release-cli-test");
        fs::create_dir_all(root.join("artifacts")).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/bundle");
        for name in [
            "ci-run.json",
            "ci.log",
            "release-plan.json",
            "artifact-index.json",
            "artifacts/spectral-cli-1.4.2.zip",
        ] {
            fs::copy(fixtures.join(name), root.join(name)).unwrap();
        }
        // The temp root is test-owned; its platform alias is not an input symlink.
        Self(root.canonicalize().unwrap())
    }
    fn update(&self, name: &str, f: impl FnOnce(&mut Value)) {
        let path = self.0.join(name);
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        f(&mut value);
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    fn check(&self) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_release-wraith"))
            .arg("check")
            .arg(&self.0)
            .output()
            .unwrap();
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            out.status.code().unwrap(),
            serde_json::from_slice(&out.stdout).unwrap(),
        )
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn failed_contract_exits_one_without_modifying_inputs() {
    let b = Bundle::new();
    let before = fs::read(b.0.join("release-plan.json")).unwrap();
    let (code, report) = b.check();
    assert_eq!(code, 1);
    assert_eq!(report["status"], "FAIL");
    assert_eq!(report["source"], "SYNTHETIC FIXTURE");
    assert_eq!(report["ci_rerun"], false);
    assert_eq!(fs::read(b.0.join("release-plan.json")).unwrap(), before);
}
#[test]
fn valid_local_contract_exits_zero_without_claiming_live_ci_passed() {
    let b = Bundle::new();
    b.update("release-plan.json", |p| {
        p["artifact"] = json!("artifacts/spectral-cli-1.4.2.zip")
    });
    let (code, report) = b.check();
    assert_eq!(code, 0);
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["historical_ci_conclusion"], "failure");
    assert_eq!(report["artifact_present_and_sha256_verified"], true);
    assert_eq!(report["ci_rerun"], false);
    assert_eq!(report["published"], false);
}
#[test]
fn tampered_local_artifact_is_invalid_and_exits_two() {
    let b = Bundle::new();
    fs::write(b.0.join("artifacts/spectral-cli-1.4.2.zip"), b"changed").unwrap();
    let (code, report) = b.check();
    assert_eq!(code, 2);
    assert_eq!(report["status"], "INVALID");
    assert!(report["error"].as_str().unwrap().contains("do not match"));
}
#[test]
fn exported_user_bundle_is_accepted_without_a_model_key() {
    let b = Bundle::new();
    for name in ["ci-run.json", "release-plan.json", "artifact-index.json"] {
        b.update(name, |p| p["fixture"] = json!(false));
    }
    let (code, report) = b.check();
    assert_eq!(code, 1);
    assert_eq!(
        report["source"],
        "USER-SUPPLIED LOCAL EXPORT; NOT INDEPENDENTLY FETCHED"
    );
}
#[test]
fn invalid_usage_is_json_and_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_release-wraith"))
        .arg("check")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], "INVALID");
}

#[test]
fn bridge_cli_exports_real_check_and_never_overwrites() {
    let b = Bundle::new();
    let output = b.0.join("handoff.json");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_release-wraith"))
            .arg("export-bridge")
            .arg(&b.0)
            .arg(&output)
            .output()
            .unwrap()
    };
    let first = invoke();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let packet = wraithkit::bridge::decode::<Value>(
        &bytes,
        "release-wraith",
        "research-wraith",
        "release-contract-report",
    )
    .unwrap();
    assert_eq!(packet.packet().payload["status"], "FAIL");
    assert_eq!(invoke().status.code(), Some(2));
    assert_eq!(fs::read(output).unwrap(), bytes);
}
