use super::*;
use wraithkit::{
    ledger::{ClaimStatus, Event, RunStatus},
    model::{Model, tool_calls},
    session::Session,
    verify::{all_ok, verify},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let path = wraithkit::demo_dir(&format!("release-test-{tag}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn bundle() -> Self {
        Self(ReleaseWraith::demo().unwrap().target)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn open(root: &Path) -> ReleaseWraith {
    ReleaseWraith::open(root, Mode::Fixture).unwrap()
}
fn fix() -> Value {
    json!({"from":"dist/spectral-cli.zip","to":"artifacts/spectral-cli-1.4.2.zip"})
}
fn session(root: &Path, runs: &Path, script: &str) -> Session<ReleaseWraith> {
    Session::start(
        runs,
        root,
        TASK,
        Mode::Fixture,
        Model::fixture(script).unwrap(),
    )
    .unwrap()
}
fn drive(s: &mut Session<ReleaseWraith>) {
    for _ in 0..30 {
        if s.state.status != Some(RunStatus::Running) {
            return;
        }
        s.step().unwrap();
    }
    panic!("fixture did not settle");
}
fn one_call(name: &str, arguments: Value) -> String {
    json!([
        {"role":"assistant","content":null,"tool_calls":[{"id":"single","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]},
        {"role":"assistant","content":"Stopped. No hosted CI ran."}
    ]).to_string()
}
fn digest(s: &Session<ReleaseWraith>) -> String {
    s.state.proposals[0].digest.clone()
}
fn bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    DEMO_FILES
        .iter()
        .map(|(name, _)| (name.to_string(), fs::read(root.join(name)).unwrap()))
        .collect()
}
fn applied(s: &Session<ReleaseWraith>) -> usize {
    s.ledger
        .lines
        .iter()
        .filter(|l| matches!(l.event, Event::EffectDone { ok: true, .. }))
        .count()
}
fn result(s: &Session<ReleaseWraith>, id: &str) -> (bool, String) {
    s.ledger
        .lines
        .iter()
        .find_map(|l| match &l.event {
            Event::ToolResult {
                call_id,
                ok,
                content,
                ..
            } if call_id == id => Some((*ok, content.clone())),
            _ => None,
        })
        .unwrap()
}
fn assert_verified(s: &Session<ReleaseWraith>) {
    let (checks, _) = verify(s.run_dir()).unwrap();
    assert!(all_ok(&checks), "ledger verification failed");
}

#[test]
fn demo_waits_for_exact_approval_without_writing() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("waiting");
    let before = bytes(bundle.path());
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Waiting));
    assert_eq!(s.state.proposals.len(), 1);
    assert_eq!(s.state.proposals[0].id, "P1");
    assert_eq!(s.state.proposals[0].tool, "fix_artifact_path");
    assert!(s.state.model.starts_with("FIXTURE"));
    assert_eq!(bytes(bundle.path()), before);
    assert_eq!(applied(&s), 0);
    assert!(
        s.state.proposals[0]
            .detail
            .contains("+   \"artifact\": \"artifacts/spectral-cli-1.4.2.zip\"")
    );
    assert!(
        s.state.proposals[0]
            .detail
            .contains("-   \"artifact\": \"dist/spectral-cli.zip\"")
    );
    assert_eq!(s.state.proposals[0].inputs.len(), 10);
    assert!(
        s.state.proposals[0]
            .warnings
            .iter()
            .any(|w| w.contains("does not run a workflow"))
    );
    assert_verified(&s);
}

#[test]
fn approval_changes_only_the_plan_and_verifies_real_local_bytes() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("approve");
    let before = bytes(bundle.path());
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    assert_eq!(s.decide("P1", true, &digest(&s)).unwrap(), "approved");
    drive(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Finished));
    assert_eq!(applied(&s), 1);
    let after = bytes(bundle.path());
    for (name, content) in &before {
        if name != "release-plan.json" {
            assert_eq!(after[name], *content, "{name} changed");
        }
    }
    let old: ReleasePlan = serde_json::from_slice(&before["release-plan.json"]).unwrap();
    let new: ReleasePlan = serde_json::from_slice(&after["release-plan.json"]).unwrap();
    assert_eq!(
        ReleasePlan {
            artifact: new.artifact.clone(),
            ..old
        },
        new
    );
    assert_eq!(new.artifact, "artifacts/spectral-cli-1.4.2.zip");
    let check = check_target(bundle.path()).unwrap();
    assert_eq!(check["status"], "PASS");
    assert_eq!(check["historical_ci_conclusion"], "failure");
    assert_eq!(check["ci_rerun"], false);
    assert_eq!(check["published"], false);
    assert_eq!(check["artifact_present_and_sha256_verified"], true);
    assert!(result(&s, "after").1.contains("\"status\": \"PASS\""));
    assert_verified(&s);
    assert!(fs::read_dir(bundle.path()).unwrap().all(|p| {
        !p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".release-wraith-")
    }));
}

#[test]
fn denial_never_writes_and_the_final_check_remains_failed() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("deny");
    let before = bytes(bundle.path());
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    assert_eq!(s.decide("P1", false, &digest(&s)).unwrap(), "denied");
    drive(&mut s);
    assert_eq!(bytes(bundle.path()), before);
    assert_eq!(applied(&s), 0);
    assert!(result(&s, "after").1.contains("\"status\": \"FAIL\""));
    assert_eq!(check_target(bundle.path()).unwrap()["status"], "FAIL");
    assert_verified(&s);
}

#[test]
fn a_wrong_digest_and_repeated_decision_cannot_apply_twice() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("digest");
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    assert!(s.decide("P1", true, "not-the-card-digest").is_err());
    assert_eq!(applied(&s), 0);
    let dg = digest(&s);
    s.decide("P1", true, &dg).unwrap();
    assert!(s.decide("P1", true, &dg).is_err());
    assert_eq!(applied(&s), 1);
}

#[test]
fn every_document_is_an_approval_dependency_even_if_only_whitespace_changed() {
    for name in FIXED {
        let bundle = Scratch::bundle();
        let runs = Scratch::new("stale-doc");
        let mut s = session(bundle.path(), runs.path(), SCRIPT);
        drive(&mut s);
        let mut changed = fs::read(bundle.path().join(name)).unwrap();
        changed.push(b'\n');
        fs::write(bundle.path().join(name), changed).unwrap();
        let before = bytes(bundle.path());
        let decision = s.decide("P1", true, &digest(&s)).unwrap();
        assert!(decision.starts_with("stale:"), "{name}: {decision}");
        assert_eq!(bytes(bundle.path()), before);
        assert_eq!(applied(&s), 0);
    }
}

#[test]
fn artifact_tampering_makes_approval_stale() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("stale-archive");
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    fs::write(
        bundle.path().join("artifacts/spectral-cli-1.4.2.zip"),
        b"changed payload",
    )
    .unwrap();
    let before = bytes(bundle.path());
    assert!(
        s.decide("P1", true, &digest(&s))
            .unwrap()
            .starts_with("stale:")
    );
    assert_eq!(bytes(bundle.path()), before);
    assert_eq!(applied(&s), 0);
}

#[test]
fn coherent_inventory_and_artifact_replacement_still_invalidates_approval() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("stale-coherent");
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    let archive = b"entirely different local artifact";
    fs::write(
        bundle.path().join("artifacts/spectral-cli-1.4.2.zip"),
        archive,
    )
    .unwrap();
    let p = bundle.path().join("artifact-index.json");
    let mut index: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    index["artifacts"][0]["bytes"] = json!(archive.len());
    index["artifacts"][0]["sha256"] = json!(sha256_hex(archive));
    fs::write(p, serde_json::to_vec_pretty(&index).unwrap()).unwrap();
    assert!(open(bundle.path()).snapshot().is_ok());
    assert!(
        s.decide("P1", true, &digest(&s))
            .unwrap()
            .starts_with("stale:")
    );
    assert_eq!(applied(&s), 0);
}

#[test]
fn resume_preserves_the_same_approval_scope() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("resume");
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    let dir = s.run_dir().to_path_buf();
    let dg = digest(&s);
    drop(s);
    let mut resumed =
        Session::<ReleaseWraith>::resume(&dir, Model::fixture(SCRIPT).unwrap()).unwrap();
    assert_eq!(digest(&resumed), dg);
    resumed.decide("P1", true, &dg).unwrap();
    drive(&mut resumed);
    assert_eq!(applied(&resumed), 1);
    assert_verified(&resumed);
}

#[test]
fn previews_bind_absolute_destinations_even_for_identical_bundles() {
    let one = Scratch::bundle();
    let two = Scratch::bundle();
    let a = open(one.path())
        .preview("fix_artifact_path", &fix())
        .unwrap();
    let b = open(two.path())
        .preview("fix_artifact_path", &fix())
        .unwrap();
    for name in FIXED {
        assert_eq!(
            a.inputs[&format!("file:{name}")],
            b.inputs[&format!("file:{name}")]
        );
        assert_ne!(
            a.inputs[&format!("destination:{name}")],
            b.inputs[&format!("destination:{name}")]
        );
    }
}

fn dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .is_ok_and(|o| o.status.success())
    }
}
fn remove_link(link: &Path) {
    fs::remove_dir(link)
        .or_else(|_| fs::remove_file(link))
        .unwrap();
}

#[test]
fn artifact_directory_symlink_or_junction_is_refused() {
    let bundle = Scratch::bundle();
    let external = Scratch::new("outside-artifacts");
    fs::write(
        external.path().join("spectral-cli-1.4.2.zip"),
        DEMO_FILES[4].1,
    )
    .unwrap();
    let originals = bundle.path().join("artifacts");
    fs::rename(&originals, bundle.path().join("original-artifacts")).unwrap();
    if !dir_link(external.path(), &originals) {
        eprintln!("SKIP: OS did not allow a directory symlink/junction");
        return;
    }
    let error = open(bundle.path()).snapshot().err().unwrap();
    assert!(error.contains("links and reparse"), "{error}");
    remove_link(&originals);
}

#[test]
fn resumed_root_alias_retarget_cannot_redirect_an_approved_write() {
    let one = Scratch::bundle();
    let two = Scratch::bundle();
    let links = Scratch::new("alias");
    let runs = Scratch::new("alias-runs");
    let alias = links.path().join("selected");
    if !dir_link(one.path(), &alias) {
        eprintln!("SKIP: OS did not allow a directory symlink/junction");
        return;
    }
    let mut s = session(&alias, runs.path(), SCRIPT);
    drive(&mut s);
    let dir = s.run_dir().to_path_buf();
    let dg = digest(&s);
    drop(s);
    remove_link(&alias);
    assert!(dir_link(two.path(), &alias));
    let before = bytes(two.path());
    let mut resumed =
        Session::<ReleaseWraith>::resume(&dir, Model::fixture(SCRIPT).unwrap()).unwrap();
    assert!(
        resumed
            .decide("P1", true, &dg)
            .unwrap()
            .starts_with("stale:")
    );
    assert_eq!(bytes(two.path()), before);
    assert_eq!(applied(&resumed), 0);
    remove_link(&alias);
}

#[test]
fn atomic_replacement_does_not_modify_an_external_hardlink() {
    let bundle = Scratch::bundle();
    let outside = Scratch::new("hardlink");
    let runs = Scratch::new("hardlink-runs");
    let external = outside.path().join("keep.json");
    fs::hard_link(bundle.path().join("release-plan.json"), &external).unwrap();
    let old = fs::read(&external).unwrap();
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    s.decide("P1", true, &digest(&s)).unwrap();
    assert_eq!(fs::read(external).unwrap(), old);
    assert_eq!(check_target(bundle.path()).unwrap()["status"], "PASS");
}

#[test]
fn no_arbitrary_path_secret_workflow_or_shell_tool_exists() {
    let bundle = Scratch::bundle();
    let app = open(bundle.path());
    assert_eq!(
        app.tools()
            .iter()
            .filter(|t| t.kind == Kind::Effect)
            .map(|t| t.name)
            .collect::<Vec<_>>(),
        vec!["fix_artifact_path"]
    );
    for name in [
        ".env",
        "../ci.log",
        "C:/secret",
        "/etc/passwd",
        ".github/workflows/ci.yml",
        "artifacts/x",
    ] {
        let runs = Scratch::new("blocked-read");
        let mut s = session(
            bundle.path(),
            runs.path(),
            &one_call("read_release_input", json!({"name":name})),
        );
        drive(&mut s);
        assert!(!result(&s, "single").0, "{name}");
    }
    for name in ["exec", "shell", "run_ci", "push", "publish", "edit_file"] {
        assert!(app.preview(name, &fix()).is_err());
    }
}

#[test]
fn hostile_artifact_paths_are_rejected_portably() {
    for name in [
        "",
        "../x",
        "/tmp/a",
        "C:/x",
        "artifacts/../x",
        "artifacts/./x",
        "artifacts//x",
        "artifacts/x.",
        "artifacts/CON",
        "artifacts/nul.txt",
        "artifacts/COM1.zip",
        "artifacts/x:y",
        "artifacts\\a.zip",
        "artifacts/x\u{202e}zip",
        "artifacts/a b.zip",
        "artifacts/~x",
    ] {
        assert!(artifact_name(name).is_err(), "{name}");
    }
    assert!(artifact_name("artifacts/linux-x86_64/app-1.2.zip").is_ok());
}

#[test]
fn effect_arguments_cannot_select_any_other_file_or_field() {
    let bundle = Scratch::bundle();
    let app = open(bundle.path());
    for a in [
        json!({"from":"dist/spectral-cli.zip","to":"artifacts/spectral-cli-1.4.2.zip","path":".env"}),
        json!({"from":"other","to":"artifacts/spectral-cli-1.4.2.zip"}),
        json!({"from":"dist/spectral-cli.zip","to":"dist/spectral-cli.zip"}),
        json!({"from":"dist/spectral-cli.zip","to":"artifacts/not-present.zip"}),
        json!({"from":"dist/spectral-cli.zip","to":"../outside"}),
        json!({"from":5,"to":"artifacts/spectral-cli-1.4.2.zip"}),
    ] {
        assert!(app.preview("fix_artifact_path", &a).is_err(), "{a}");
    }
}

#[test]
fn unknown_probe_arguments_are_refused() {
    let bundle = Scratch::bundle();
    for (tool, a) in [
        ("read_ci_failure", json!({"command":"ignored"})),
        ("check_release_contract", json!({"approve":true})),
        (
            "read_release_input",
            json!({"name":"release-plan.json","path":"elsewhere"}),
        ),
    ] {
        let runs = Scratch::new("unknown-args");
        let mut s = session(bundle.path(), runs.path(), &one_call(tool, a));
        drive(&mut s);
        let (ok, note) = result(&s, "single");
        assert!(!ok && note.contains("invalid arguments"));
    }
}

#[test]
fn size_encoding_and_regular_file_guards_hold() {
    let bundle = Scratch::bundle();
    let app = open(bundle.path());
    fs::write(
        bundle.path().join("ci.log"),
        vec![b'x'; TEXT_LIMIT as usize + 1],
    )
    .unwrap();
    assert!(app.read_text("ci.log").err().unwrap().contains("at most"));
    fs::write(bundle.path().join("ci.log"), [0xff, 0xfe]).unwrap();
    assert!(app.read_text("ci.log").err().unwrap().contains("not UTF-8"));
    fs::remove_file(bundle.path().join("ci.log")).unwrap();
    fs::create_dir(bundle.path().join("ci.log")).unwrap();
    assert!(app.read_text("ci.log").is_err());
}

#[test]
fn inventory_metadata_does_not_substitute_for_actual_artifact_bytes() {
    let bundle = Scratch::bundle();
    fs::remove_file(bundle.path().join("artifacts/spectral-cli-1.4.2.zip")).unwrap();
    assert!(check_target(bundle.path()).is_err());
    fs::write(
        bundle.path().join("artifacts/spectral-cli-1.4.2.zip"),
        b"not the archive",
    )
    .unwrap();
    assert!(
        check_target(bundle.path())
            .err()
            .unwrap()
            .contains("do not match")
    );
}

#[test]
fn malformed_duplicate_or_oversized_inventory_is_rejected() {
    for kind in [
        "unknown",
        "duplicate",
        "empty",
        "too-many",
        "huge",
        "hash",
        "wrong-source",
    ] {
        let bundle = Scratch::bundle();
        let path = bundle.path().join("artifact-index.json");
        let mut index: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let entry = index["artifacts"][0].clone();
        match kind {
            "unknown" => index["unexpected"] = json!(true),
            "duplicate" => index["artifacts"] = json!([entry.clone(), entry]),
            "empty" => index["artifacts"] = json!([]),
            "too-many" => index["artifacts"] = json!(vec![entry; 17]),
            "huge" => index["artifacts"][0]["bytes"] = json!(ARTIFACT_LIMIT + 1),
            "hash" => index["artifacts"][0]["sha256"] = json!("not-a-hash"),
            "wrong-source" => index["fixture"] = json!(false),
            _ => unreachable!(),
        }
        fs::write(path, serde_json::to_vec(&index).unwrap()).unwrap();
        assert!(check_target(bundle.path()).is_err(), "{kind}");
    }
}

#[test]
fn local_export_mode_is_neither_synthetic_nor_authenticated_remote_evidence() {
    let bundle = Scratch::bundle();
    for name in ["ci-run.json", "release-plan.json", "artifact-index.json"] {
        let path = bundle.path().join(name);
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["fixture"] = json!(false);
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    let check = check_target(bundle.path()).unwrap();
    assert_eq!(check["status"], "FAIL");
    assert!(
        check["source"]
            .as_str()
            .unwrap()
            .contains("USER-SUPPLIED LOCAL EXPORT")
    );
    assert!(
        check["source"]
            .as_str()
            .unwrap()
            .contains("NOT INDEPENDENTLY FETCHED")
    );
    assert_eq!(check["ci_rerun"], false);
    assert!(
        ReleaseWraith::open(bundle.path(), Mode::Live)
            .unwrap()
            .preview("fix_artifact_path", &fix())
            .is_ok()
    );
}

#[test]
fn the_untrusted_log_annotation_is_evidence_not_an_approval() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("injection");
    let before = bytes(bundle.path());
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    assert!(
        s.state
            .evidence
            .iter()
            .any(|e| e.source == "local-release:ci.log"
                && e.flags.iter().any(|f| f == "addresses_ai_agents"))
    );
    assert_eq!(s.state.proposals.len(), 1);
    assert_eq!(applied(&s), 0);
    assert_eq!(bytes(bundle.path()), before);
    assert!(s.ledger.lines.iter().any(|l|matches!(&l.event,Event::Claim {status:ClaimStatus::Supported,cites,..} if cites.len()==3 && cites.iter().all(|c|c.found))));
}

#[test]
fn the_fixture_script_has_only_known_well_formed_tools() {
    let bundle = Scratch::bundle();
    let app = open(bundle.path());
    let tools = app.tools();
    let names = tools
        .iter()
        .map(|t| t.name)
        .chain(["claim", "finish"])
        .collect::<Vec<_>>();
    let turns: Vec<Value> = serde_json::from_str(SCRIPT).unwrap();
    for turn in turns {
        for call in tool_calls(&turn) {
            assert!(names.contains(&call.name.as_str()));
            assert!(call.args.is_ok());
        }
    }
}

#[test]
fn exact_preview_and_apply_preserve_crlf() {
    let bundle = Scratch::bundle();
    let runs = Scratch::new("crlf");
    let path = bundle.path().join("release-plan.json");
    let original = fs::read_to_string(&path).unwrap().replace('\n', "\r\n");
    fs::write(&path, original).unwrap();
    let mut s = session(bundle.path(), runs.path(), SCRIPT);
    drive(&mut s);
    s.decide("P1", true, &digest(&s)).unwrap();
    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("\r\n"));
    assert!(!content.replace("\r\n", "").contains('\n'));
}

#[cfg(unix)]
#[test]
fn input_file_symlink_is_refused() {
    let bundle = Scratch::bundle();
    let outside = Scratch::new("outside-file");
    let original = bundle.path().join("release-plan.json");
    let copy = outside.path().join("plan.json");
    fs::rename(&original, &copy).unwrap();
    std::os::unix::fs::symlink(&copy, &original).unwrap();
    assert!(
        open(bundle.path())
            .preview("fix_artifact_path", &fix())
            .err()
            .unwrap()
            .contains("links")
    );
}
