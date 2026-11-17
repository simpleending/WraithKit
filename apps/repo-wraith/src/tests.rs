//! Repo Wraith tests: path and secret guards, previews, and effects run through a real `Session`.

use super::*;
use crate::guard::{MAX_FILE_BYTES, is_build_or_ci, is_secret_name};
use serde_json::json;
use wraithkit::now_ms;
use wraithkit::{
    ledger::{Event, RunStatus},
    model::{Model, tool_calls},
    session::Session,
    verify::{all_ok, verify},
};

/// A folder removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "repo-wraith-test-{tag}-{}-{}-{}",
            std::process::id(),
            now_ms(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Scratch(p)
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

fn demo_repo() -> Scratch {
    Scratch(RepoWraith::demo().unwrap().target)
}

fn open(p: &Path) -> RepoWraith {
    RepoWraith::open(p, Mode::Fixture).unwrap()
}

fn make_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(target, link).is_ok();
    made
}

/// A directory link. On Windows this is a junction, which needs no special privilege.
fn make_dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link).is_ok();
    #[cfg(windows)]
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|o| o.status.success());
    made
}

#[test]
fn link_into_git_refused() {
    let d = Scratch::new("gitlink");
    fs::create_dir_all(d.path().join(".git")).unwrap();
    fs::write(
        d.path().join(".git/config"),
        "url = https://token@example.com/x",
    )
    .unwrap();
    if !make_dir_link(&d.path().join(".git"), &d.path().join("g")) {
        eprintln!("skipped link_into_git_refused: this OS refused to create a directory link");
        return;
    }
    let app = open(d.path());
    let err = app.read_text("g/config").unwrap_err();
    assert!(err.contains("off limits"), "{err}");
}

#[test]
fn own_run_folder_is_off_limits() {
    // The run folder sits inside the repo when the tool is started from the repo root.
    let d = demo_repo();
    fs::create_dir_all(d.path().join(".wraith/runs/x")).unwrap();
    fs::write(d.path().join(".wraith/runs/x/ledger.jsonl"), "{}\n").unwrap();
    let app = open(d.path());
    for p in [".wraith/runs/x/ledger.jsonl", ".WRAITH/runs/x/ledger.jsonl"] {
        assert!(app.read_text(p).unwrap_err().contains("off limits"), "{p}");
    }
    let edit = json!({"path": ".wraith/runs/x/ledger.jsonl", "find": "{}", "replace": "x"});
    assert!(
        app.preview("edit_file", &edit).is_err(),
        "the ledger cannot be edited"
    );
}

fn session(target: &Path, runs: &Path, script: &str) -> Session<RepoWraith> {
    Session::start(
        runs,
        target,
        TASK,
        Mode::Fixture,
        Model::fixture(script).unwrap(),
    )
    .unwrap()
}

fn run_to_end(s: &mut Session<RepoWraith>) {
    for _ in 0..100 {
        if s.state.status != Some(RunStatus::Running) {
            break;
        }
        s.step().unwrap();
    }
}

fn digest_of(s: &Session<RepoWraith>, id: &str) -> String {
    s.state
        .proposals
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .digest
        .clone()
}

fn tool_content(s: &Session<RepoWraith>, call_id: &str) -> (bool, String) {
    s.ledger
        .lines
        .iter()
        .find_map(|l| match &l.event {
            Event::ToolResult {
                call_id: c,
                ok,
                content,
                ..
            } if c == call_id => Some((*ok, content.clone())),
            _ => None,
        })
        .unwrap()
}

/// One assistant turn with a single tool call, then a text reply.
fn one_call(name: &str, args: serde_json::Value) -> String {
    json!([
        {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": name, "arguments": args.to_string()}}]},
        {"role": "assistant", "content": "done"}
    ])
    .to_string()
}

#[test]
fn paths() {
    let root = Scratch::new("paths");
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("ok.txt"), "x").unwrap();
    fs::write(root.path().join("src/lib.rs"), "x").unwrap();
    let r = root.path().canonicalize().unwrap();
    for bad in [
        "",
        "/etc/passwd",
        "C:x",
        "C:\\Windows",
        "..",
        "../x",
        "a/../b",
        ".",
        "./ok.txt",
        "a:stream",
        ".git/config",
        "GIT/x",
        "src/../ok.txt",
        "\\\\server\\share\\x",
    ] {
        assert!(resolve_in(&r, bad).is_err(), "{bad:?} must be refused");
    }
    assert!(resolve_in(&r, "ok.txt").is_ok());
    assert!(resolve_in(&r, "src/lib.rs").is_ok());
    assert!(resolve_in(&r, "missing.txt").is_err());
}

#[test]
fn refuses_secret_files() {
    let d = demo_repo();
    let app = open(d.path());
    assert!(
        app.read_text(".env").unwrap_err().starts_with("blocked:"),
        ".env is in the demo and refused"
    );
    for name in [
        ".env.local",
        "id_rsa",
        "id_ed25519.pub",
        "server.pem",
        "cred.key",
        "credentials.json",
        "secrets.yml",
        ".npmrc",
        "x.p12",
    ] {
        fs::write(app.root.join(name), "placeholder").unwrap();
        assert!(app.read_text(name).is_err(), "{name} must be refused");
    }
    for name in ["README.md", "envoy.txt", "keys.md", "secret_notes.md"] {
        assert!(!is_secret_name(name), "{name} is not a secret name");
    }
}

#[test]
fn refuses_large_and_binary() {
    let d = Scratch::new("large");
    let app = open(d.path());
    fs::write(
        d.path().join("big.txt"),
        vec![b'a'; MAX_FILE_BYTES as usize + 1],
    )
    .unwrap();
    assert!(
        app.read_text("big.txt")
            .unwrap_err()
            .contains("larger than 256 KB")
    );
    fs::write(d.path().join("bin.dat"), [0xff, 0xfe, 0x00, 0x81]).unwrap();
    assert!(app.read_text("bin.dat").unwrap_err().contains("not UTF-8"));
    fs::write(d.path().join("ok.txt"), "plain text").unwrap();
    assert!(app.read_text("ok.txt").is_ok());
}

#[test]
fn symlink_escape_refused() {
    let d = Scratch::new("link");
    let outside = Scratch::new("outside");
    fs::write(outside.path().join("notes.txt"), "outside the repository").unwrap();
    if !make_link(
        &outside.path().join("notes.txt"),
        &d.path().join("link.txt"),
    ) {
        eprintln!("skipped symlink_escape_refused: this OS refused to create a symlink");
        return;
    }
    let app = open(d.path());
    assert!(
        app.read_text("link.txt").is_err(),
        "a symlink out of the repo is refused"
    );
    assert!(
        !app.files().iter().any(|f| f == "link.txt"),
        "symlinks are not listed"
    );
}

#[test]
fn tools_have_no_network_or_shell() {
    let app = RepoWraith {
        root: PathBuf::new(),
    };
    let names: Vec<&str> = app.tools().iter().map(|t| t.name).collect();
    assert_eq!(names, ["list_files", "read_file", "search", "edit_file"]);
    assert_eq!(
        app.tools()
            .iter()
            .filter(|t| t.kind == Kind::Effect)
            .count(),
        1,
        "one effect: edit_file"
    );
}

#[test]
fn edit_requires_unique_match() {
    let d = demo_repo();
    let app = open(d.path());
    let edit = |find: &str| json!({"path": "invoice/money.py", "find": find, "replace": "x"});
    assert!(
        app.preview("edit_file", &edit("amount"))
            .unwrap_err()
            .contains("matched 2 times")
    );
    assert!(
        app.preview("edit_file", &edit("nowhere"))
            .unwrap_err()
            .contains("matched 0 times")
    );
    assert!(app.preview("edit_file", &edit("int(amount * 100)")).is_ok());
    assert!(
        app.preview("edit_file", &edit("")).is_err(),
        "an empty find is refused"
    );
}

#[test]
fn ci_edit_warns() {
    let d = demo_repo();
    let app = open(d.path());
    let ci = app
        .preview("edit_file", &json!({"path": ".github/workflows/ci.yml", "find": "      - run: pytest", "replace": "x"}))
        .unwrap();
    assert!(
        ci.warnings.iter().any(|w| w.contains("CI")),
        "CI files warn"
    );
    let code = app
        .preview(
            "edit_file",
            &json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "x"}),
        )
        .unwrap();
    assert!(code.warnings.is_empty());
    for p in [
        "Makefile",
        "build.rs",
        "package.json",
        "ci/workflow.yml",
        ".github\\actions\\setup.yml",
        ".GITHUB/x.yml",
    ] {
        assert!(is_build_or_ci(p), "{p} warns");
    }
}

#[test]
fn list_files_skips_secrets_and_git() {
    let d = demo_repo();
    fs::create_dir_all(d.path().join(".git")).unwrap();
    fs::write(d.path().join(".git/config"), "x").unwrap();
    let runs = Scratch::new("runs-list");
    let mut s = session(d.path(), runs.path(), &one_call("list_files", json!({})));
    run_to_end(&mut s);
    let (ok, listing) = tool_content(&s, "c1");
    assert!(ok);
    assert!(listing.contains("README.md (") && listing.contains("invoice/money.py ("));
    assert!(
        !listing.contains(".env (") && !listing.lines().any(|l| l.starts_with(".git/")),
        "secrets and .git are not listed: {listing}"
    );
}

#[test]
fn search_finds_literal_hits() {
    let d = demo_repo();
    let runs = Scratch::new("runs-search");
    let mut s = session(
        d.path(),
        runs.path(),
        &one_call("search", json!({"text": "int(amount * 100)"})),
    );
    run_to_end(&mut s);
    let (ok, hits) = tool_content(&s, "c1");
    assert!(ok);
    let mut out = hits.lines();
    assert_eq!(out.next(), Some("[E1] 1 hits"), "{hits}");
    assert!(
        out.next()
            .is_some_and(|l| l.starts_with("invoice/money.py:")),
        "{hits}"
    );
}

#[test]
fn edit_applies_after_approval() {
    let d = demo_repo();
    let runs = Scratch::new("runs-apply");
    let script = one_call(
        "edit_file",
        json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "round(amount * 100)"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let dg = digest_of(&s, "P1");
    s.decide("P1", true, &dg).unwrap();
    let money = fs::read_to_string(d.path().join("invoice/money.py")).unwrap();
    assert!(money.contains("round(amount * 100)"));
}

#[test]
fn stale_input_blocks_effect() {
    let d = demo_repo();
    let runs = Scratch::new("runs-stale");
    let script = one_call(
        "edit_file",
        json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "round(amount * 100)"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let dg = digest_of(&s, "P1");
    let money_path = d.path().join("invoice/money.py");
    let before = fs::read_to_string(&money_path).unwrap();
    fs::write(&money_path, format!("{before}# edited in another editor\n")).unwrap();
    let outcome = s.decide("P1", true, &dg).unwrap();
    assert!(
        outcome.starts_with("stale:") && outcome.contains("invoice/money.py"),
        "{outcome}"
    );
    assert!(
        !fs::read_to_string(&money_path).unwrap().contains("round("),
        "the stale edit did not run"
    );
}

#[test]
fn script_is_valid() {
    let turns: Vec<serde_json::Value> = serde_json::from_str(SCRIPT).expect("script.json parses");
    let app = RepoWraith {
        root: PathBuf::new(),
    };
    let mut known: Vec<String> = app.tools().iter().map(|t| t.name.to_string()).collect();
    known.extend(["claim".to_string(), "finish".to_string()]);
    for turn in &turns {
        for c in tool_calls(turn) {
            assert!(known.contains(&c.name), "unknown tool {} in script", c.name);
            assert!(c.args.is_ok(), "arguments of {} are valid JSON", c.name);
        }
    }
}

#[test]
fn demo_runs_offline() {
    let demo = RepoWraith::demo().unwrap();
    let d = Scratch(demo.target.clone());
    let runs = Scratch::new("runs-demo");
    let mut s = Session::<RepoWraith>::start(
        runs.path(),
        d.path(),
        TASK,
        Mode::Fixture,
        Model::fixture(RepoWraith::DEMO_SCRIPT).unwrap(),
    )
    .unwrap();
    assert!(
        s.state.model.starts_with("FIXTURE"),
        "the fixture label is recorded"
    );
    run_to_end(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Waiting));
    assert_eq!(s.state.proposals.len(), 2, "one fix, one CI edit");
    assert!(
        s.state.proposals[1]
            .warnings
            .iter()
            .any(|w| w.contains("CI")),
        "the CI edit carries a warning"
    );
    assert!(
        s.state.proposals[0].influenced_by.iter().any(|e| e == "E1"),
        "README evidence influenced the proposal"
    );
    assert!(
        s.state.evidence[0]
            .flags
            .iter()
            .any(|f| f == "addresses_ai_agents"),
        "the README note is flagged"
    );

    let p2 = digest_of(&s, "P2");
    assert_eq!(s.decide("P2", false, &p2).unwrap(), "denied");
    let p1 = digest_of(&s, "P1");
    assert_eq!(s.decide("P1", true, &p1).unwrap(), "approved");
    run_to_end(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Finished));

    let money = fs::read_to_string(d.path().join("invoice/money.py")).unwrap();
    assert!(
        money.contains("round(amount * 100)"),
        "the approved fix is applied"
    );
    let ci = fs::read_to_string(d.path().join(".github/workflows/ci.yml")).unwrap();
    assert_eq!(
        ci, DEMO_FILES[4].1,
        "the denied CI edit did not change the file"
    );
    assert!(
        s.ledger
            .lines
            .iter()
            .any(|l| matches!(&l.event, Event::Blocked { tool, reason, .. }
            if tool == "read_file" && reason.contains(".env"))),
        "the .env read is recorded as blocked"
    );
    let (checks, _) = verify(s.run_dir()).unwrap();
    assert!(all_ok(&checks), "verify passes on the demo run");
}

#[test]
fn edit_matches_multiline_find_in_crlf_file() {
    let d = Scratch::new("crlf");
    fs::write(d.path().join("a.py"), "def f():\r\n    return 1\r\n").unwrap();
    let app = open(d.path());
    // read_file shows lines without \r, so the model's multi-line find uses \n.
    let edit = json!({"path": "a.py", "find": "def f():\n    return 1", "replace": "def f():\n    return 2"});
    let p = app
        .preview("edit_file", &edit)
        .expect("the CRLF file matches");
    let digest = wraithkit::session::proposal_digest("edit_file", &edit, &p.inputs, &[]);
    let runs = Scratch::new("runs-crlf");
    let script = one_call("edit_file", edit);
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    assert_eq!(s.state.proposals[0].digest, digest);
    s.decide("P1", true, &digest).unwrap();
    assert_eq!(
        fs::read_to_string(d.path().join("a.py")).unwrap(),
        "def f():\r\n    return 2\r\n",
        "CRLF is kept"
    );
}

#[cfg(unix)]
#[test]
fn edit_keeps_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let d = Scratch::new("mode");
    let script_path = d.path().join("run.sh");
    fs::write(&script_path, "echo one\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    let runs = Scratch::new("runs-mode");
    let mut s = session(
        d.path(),
        runs.path(),
        &one_call(
            "edit_file",
            json!({"path": "run.sh", "find": "one", "replace": "two"}),
        ),
    );
    s.step().unwrap();
    let dg = digest_of(&s, "P1");
    s.decide("P1", true, &dg).unwrap();
    assert_eq!(fs::read_to_string(&script_path).unwrap(), "echo two\n");
    assert_eq!(
        fs::metadata(&script_path).unwrap().permissions().mode() & 0o777,
        0o755,
        "the executable bit survives"
    );
}

#[test]
fn edit_through_link_shows_real_path_and_warns() {
    // A directory link whose name hides that it points into .github.
    let d = demo_repo();
    if !make_dir_link(
        &d.path().join(".github").join("workflows"),
        &d.path().join("docs"),
    ) {
        eprintln!(
            "skipped edit_through_link_shows_real_path_and_warns: this OS refused to create a directory link"
        );
        return;
    }
    let app = open(d.path());
    let p = app
        .preview(
            "edit_file",
            &json!({"path": "docs/ci.yml", "find": "      - run: pytest", "replace": "x"}),
        )
        .unwrap();
    assert_eq!(
        p.summary, "Edit .github/workflows/ci.yml: replace 1 block",
        "the card names the real file"
    );
    assert!(
        p.warnings.iter().any(|w| w.contains("CI")),
        "the CI warning uses the real path"
    );
    assert!(
        p.warnings.iter().any(|w| w.contains("which is really")),
        "{:?}",
        p.warnings
    );
    assert!(p.inputs.contains_key(".github/workflows/ci.yml"));
}

#[test]
fn overlapping_find_is_not_unique() {
    let d = Scratch::new("overlap");
    fs::write(d.path().join("a.txt"), "aXaXa").unwrap();
    let app = open(d.path());
    let err = app
        .preview(
            "edit_file",
            &json!({"path": "a.txt", "find": "aXa", "replace": "b"}),
        )
        .unwrap_err();
    assert!(err.contains("more than once"), "{err}");
    assert!(
        app.preview(
            "edit_file",
            &json!({"path": "a.txt", "find": "aXaXa", "replace": "b"})
        )
        .is_ok()
    );
}

#[test]
fn search_hits_are_evidence() {
    let d = demo_repo();
    let runs = Scratch::new("runs-search-ev");
    let script = json!([
        {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "search", "arguments": "{\"text\":\"Note for AI agents\"}"}}]},
        {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c2", "type": "function", "function": {"name": "edit_file",
             "arguments": "{\"path\":\"invoice/money.py\",\"find\":\"int(amount * 100)\",\"replace\":\"round(amount * 100)\"}"}}]}
    ])
    .to_string();
    let mut s = session(d.path(), runs.path(), &script);
    run_to_end(&mut s);
    let (ok, out) = tool_content(&s, "c1");
    assert!(ok && out.starts_with("[E1] 1 hits"), "{out}");
    assert!(
        s.state.evidence[0]
            .flags
            .iter()
            .any(|f| f == "addresses_ai_agents"),
        "search text is flagged"
    );
    assert_eq!(
        s.state.proposals[0].influenced_by,
        ["E1"],
        "and it counts as influence on the card"
    );
}

#[cfg(unix)]
#[test]
fn planted_temp_link_cannot_redirect_the_write() {
    let d = demo_repo();
    let outside = Scratch::new("victim");
    let victim = outside.path().join("authorized_keys");
    fs::write(&victim, "original").unwrap();
    std::os::unix::fs::symlink(&victim, d.path().join("invoice/money.py.wraith-tmp")).unwrap();
    let runs = Scratch::new("runs-tmplink");
    let script = one_call(
        "edit_file",
        json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "round(amount * 100)"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let dg = digest_of(&s, "P1");
    s.decide("P1", true, &dg).unwrap();
    assert_eq!(
        fs::read_to_string(&victim).unwrap(),
        "original",
        "nothing was written through the link"
    );
    let money = d.path().join("invoice/money.py");
    assert!(
        !fs::symlink_metadata(&money)
            .unwrap()
            .file_type()
            .is_symlink(),
        "money.py is a real file"
    );
    assert!(
        fs::read_to_string(&money)
            .unwrap()
            .contains("round(amount * 100)")
    );
}

#[test]
fn resumed_edit_refuses_retargeted_root() {
    let d = Scratch::new("resume-root");
    let outside = Scratch::new("resume-root-outside");
    let base = d.path().join("repo");
    fs::create_dir(&base).unwrap();
    fs::write(base.join("file.txt"), "original").unwrap();
    fs::write(outside.path().join("file.txt"), "original").unwrap();
    let runs = Scratch::new("runs-resume-root");
    let script = one_call(
        "edit_file",
        json!({"path": "file.txt", "find": "original", "replace": "changed"}),
    );
    let mut s = session(&base, runs.path(), &script);
    s.step().unwrap();
    let digest = digest_of(&s, "P1");
    let run = s.run_dir().to_path_buf();
    drop(s);
    fs::rename(&base, d.path().join("original-repo")).unwrap();
    assert!(
        make_dir_link(outside.path(), &base),
        "test directory link must be created"
    );
    let mut resumed =
        Session::<RepoWraith>::resume(&run, Model::fixture(&script).unwrap()).unwrap();
    let result = resumed.decide("P1", true, &digest).unwrap();
    assert!(
        result.starts_with("stale:"),
        "retargeting the root must invalidate approval, got {result}"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("file.txt")).unwrap(),
        "original"
    );
}

#[test]
fn resumed_edit_keeps_unchanged_destination() {
    let d = demo_repo();
    let runs = Scratch::new("runs-resume-normal");
    let script = one_call(
        "edit_file",
        json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "round(amount * 100)"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let digest = digest_of(&s, "P1");
    let run = s.run_dir().to_path_buf();
    drop(s);
    let mut resumed =
        Session::<RepoWraith>::resume(&run, Model::fixture(&script).unwrap()).unwrap();
    assert_eq!(resumed.decide("P1", true, &digest).unwrap(), "approved");
    assert!(
        fs::read_to_string(d.path().join("invoice/money.py"))
            .unwrap()
            .contains("round(amount * 100)")
    );
}

#[test]
fn edit_preserves_an_unrelated_legacy_temp_named_sibling() {
    let d = demo_repo();
    let sibling = d.path().join("invoice/money.py.wraith-tmp");
    let original = b"unrelated user-owned recovery data\n";
    fs::write(&sibling, original).unwrap();
    let runs = Scratch::new("runs-temp-sibling");
    let mut s = session(
        d.path(),
        runs.path(),
        &one_call(
            "edit_file",
            json!({"path": "invoice/money.py", "find": "int(amount * 100)", "replace": "round(amount * 100)"}),
        ),
    );
    s.step().unwrap();
    let dg = digest_of(&s, "P1");
    s.decide("P1", true, &dg).unwrap();
    assert!(
        fs::read_to_string(d.path().join("invoice/money.py"))
            .unwrap()
            .contains("round(amount * 100)")
    );
    assert_eq!(
        fs::read(&sibling).unwrap(),
        original,
        "approval for money.py must not delete an unrelated sibling"
    );
    assert!(
        fs::read_dir(d.path().join("invoice"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".repo-wraith-")),
        "successful replacement leaves no temporary file"
    );
    let (checks, _) = verify(s.run_dir()).unwrap();
    assert!(all_ok(&checks));
}

#[test]
fn exclusive_temp_collision_preserves_the_winning_file_and_target() {
    let d = Scratch::new("temp-collision");
    let target = d.path().join("target.txt");
    let tmp = d.path().join("claimed.tmp");
    fs::write(&target, "approved old target").unwrap();
    fs::write(&tmp, "another writer won this name").unwrap();
    let error = replace_file(&target, &tmp, b"new target").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&target).unwrap(), "approved old target");
    assert_eq!(
        fs::read_to_string(&tmp).unwrap(),
        "another writer won this name"
    );
}

#[test]
fn exclusive_temp_collision_does_not_follow_or_remove_a_hardlink() {
    let d = Scratch::new("temp-hardlink");
    let outside = Scratch::new("temp-hardlink-outside");
    let victim = outside.path().join("untouched.txt");
    let target = d.path().join("target.txt");
    let tmp = d.path().join("planted.tmp");
    fs::write(&victim, "outside original").unwrap();
    fs::write(&target, "target original").unwrap();
    fs::hard_link(&victim, &tmp).unwrap();
    assert_eq!(
        replace_file(&target, &tmp, b"replacement")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(&victim).unwrap(), "outside original");
    assert_eq!(fs::read_to_string(&tmp).unwrap(), "outside original");
    assert_eq!(fs::read_to_string(&target).unwrap(), "target original");
}

#[cfg(unix)]
#[test]
fn exclusive_temp_collision_does_not_follow_or_remove_a_symlink() {
    let d = Scratch::new("temp-symlink");
    let outside = Scratch::new("temp-symlink-outside");
    let victim = outside.path().join("untouched.txt");
    let target = d.path().join("target.txt");
    let tmp = d.path().join("planted.tmp");
    fs::write(&victim, "outside original").unwrap();
    fs::write(&target, "target original").unwrap();
    std::os::unix::fs::symlink(&victim, &tmp).unwrap();
    assert_eq!(
        replace_file(&target, &tmp, b"replacement")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert!(fs::symlink_metadata(&tmp).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&victim).unwrap(), "outside original");
    assert_eq!(fs::read_to_string(&target).unwrap(), "target original");
}

#[test]
fn failed_replacement_cleans_only_the_temp_it_created() {
    let d = Scratch::new("temp-failure");
    let target = d.path().join("target-directory");
    let tmp = d.path().join("created.tmp");
    let sibling = d.path().join("unrelated.tmp");
    fs::create_dir(&target).unwrap();
    fs::write(&sibling, "preserve me").unwrap();
    assert!(replace_file(&target, &tmp, b"replacement").is_err());
    assert!(!tmp.exists());
    assert!(target.is_dir());
    assert_eq!(fs::read_to_string(&sibling).unwrap(), "preserve me");
}
