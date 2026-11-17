//! Optional local evidence export. It conveys no approval or authority.
use super::*;
use wraithkit::bridge::{self, Provenance};

pub(super) fn export(target: &Path, output: &Path) -> Result<String, String> {
    let app = ReleaseWraith::open(target, Mode::Fixture)?;
    let snapshot = app.snapshot()?;
    if [
        &snapshot.ci.run_id,
        &snapshot.ci.failed_step,
        &snapshot.plan.artifact,
    ]
    .iter()
    .any(|s| s.chars().any(char::is_control))
    {
        return Err("bridge report text must not contain control characters".into());
    }
    let inputs = snapshot
        .inputs
        .iter()
        .filter(|(name, _)| name.starts_with("file:"))
        .map(|(name, hash)| (name.clone(), hash.clone()))
        .collect();
    let bytes = bridge::encode(
        "release-wraith",
        "research-wraith",
        "release-contract-report",
        Provenance {
            source_kind: if snapshot.ci.fixture {
                "synthetic-fixture"
            } else {
                "user-supplied-export"
            }
            .into(),
            inputs,
        },
        &ReleaseWraith::report(&snapshot),
    )?;
    let output = std::path::absolute(output).map_err(|e| e.to_string())?;
    if !matches!(output.components().next_back(), Some(Component::Normal(_))) {
        return Err("output must name a new ordinary file".into());
    }
    let name = output
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("output name must be UTF-8")?;
    artifact_name(&format!("artifacts/{name}"))?;
    let parent = output.parent().ok_or("output parent missing")?;
    // Refuse aliases in all existing parent components, not only the final filename.
    let mut part = PathBuf::new();
    for component in parent.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err("output needs an unambiguous path".into());
        }
        part.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let meta = fs::symlink_metadata(&part).map_err(|e| format!("output parent: {e}"))?;
        if reparse(&meta) || !meta.is_dir() {
            return Err("output parent must be an ordinary directory, not a link".into());
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| format!("cannot create new handoff (never overwrites): {e}"))?;
    // Do not unlink on failure: a concurrent actor might replace our owned path.
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("handoff write failed; a partial new file may remain: {e}"))?;
    Ok(format!(
        "Wrote {} bytes to {}. Untrusted local evidence only; no approval transfers, CI rerun or publication.",
        bytes.len(),
        output.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Scratch {
        // Resolve trusted test temp aliases (for example macOS /var), not user inputs.
        Scratch(
            ReleaseWraith::demo()
                .unwrap()
                .target
                .canonicalize()
                .unwrap(),
        )
    }
    #[test]
    fn export_binds_actual_source_hashes_without_authority_or_raw_logs() {
        let d = fixture();
        let output = d.0.join("handoff.json");
        export(&d.0, &output).unwrap();
        let decoded = bridge::decode::<Value>(
            &fs::read(&output).unwrap(),
            "release-wraith",
            "research-wraith",
            "release-contract-report",
        )
        .unwrap();
        assert_eq!(decoded.packet().payload["status"], "FAIL");
        assert_eq!(decoded.packet().provenance.source_kind, "synthetic-fixture");
        for (name, hash) in &decoded.packet().provenance.inputs {
            assert!(name.starts_with("file:"));
            assert_eq!(
                *hash,
                sha256_hex(&fs::read(d.0.join(name.strip_prefix("file:").unwrap())).unwrap())
            );
        }
        let text = String::from_utf8(fs::read(&output).unwrap()).unwrap();
        assert!(!text.contains("destination:"));
        assert!(!text.contains("ignore previous"));
        assert!(!text.contains("approval"));
    }
    #[test]
    fn export_never_overwrites_and_invalid_artifact_creates_nothing() {
        let d = fixture();
        let output = d.0.join("handoff.json");
        fs::write(&output, "keep").unwrap();
        assert!(export(&d.0, &output).is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
        fs::write(d.0.join("artifacts/spectral-cli-1.4.2.zip"), "tampered").unwrap();
        let absent = d.0.join("absent.json");
        assert!(export(&d.0, &absent).is_err());
        assert!(!absent.exists());
    }
    #[test]
    fn real_approved_release_session_exports_changed_report() {
        use wraithkit::{
            ledger::RunStatus,
            model::Model,
            session::Session,
            verify::{all_ok, verify},
        };
        let d = fixture();
        let mut s = Session::<ReleaseWraith>::start(
            &d.0.join("runs"),
            &d.0,
            TASK,
            Mode::Fixture,
            Model::fixture(SCRIPT).unwrap(),
        )
        .unwrap();
        for _ in 0..20 {
            if s.state.status == Some(RunStatus::Waiting) {
                break;
            }
            s.step().unwrap();
        }
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        let output = d.0.join("after.json");
        export(&d.0, &output).unwrap();
        let packet = bridge::decode::<Value>(
            &fs::read(output).unwrap(),
            "release-wraith",
            "research-wraith",
            "release-contract-report",
        )
        .unwrap();
        assert_eq!(packet.packet().payload["status"], "PASS");
        assert_eq!(
            packet.packet().payload["historical_ci_conclusion"],
            "failure"
        );
        assert_eq!(packet.packet().payload["ci_rerun"], false);
        assert_eq!(packet.packet().payload["published"], false);
        assert!(all_ok(&verify(s.run_dir()).unwrap().0));
    }
    #[test]
    fn export_refuses_file_and_parent_links() {
        let d = fixture();
        let other = fixture();
        let linked = d.0.join("linked");
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&other.0, &linked);
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(&other.0, &linked);
        if let Err(e) = result {
            eprintln!("SKIP directory symlink unavailable: {e}");
            return;
        }
        assert!(export(&d.0, &linked.join("new.json")).is_err());
        assert!(!other.0.join("new.json").exists());
        let existing = d.0.join("handoff.json");
        #[cfg(unix)]
        std::os::unix::fs::symlink(other.0.join("keep.json"), &existing).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(other.0.join("keep.json"), &existing).unwrap();
        assert!(export(&d.0, &existing).is_err());
        assert!(!other.0.join("keep.json").exists());
    }
    #[cfg(unix)]
    #[test]
    fn exporter_refuses_fifo_input_before_opening() {
        let d = fixture();
        let input = d.0.join("ci.log");
        fs::remove_file(&input).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&input)
                .status()
                .unwrap()
                .success()
        );
        assert!(export(&d.0, &d.0.join("out.json")).is_err());
    }
}
