# WraithKit implementation tasks

How to work through these:
- Work in order. Each task lists the requirements it satisfies and a "done when" line.
- A task is done only when `cargo test --workspace` passes, plus any task-specific check listed with it.
- After each task, add a dated line to BUILD_LOG.md: the task, what changed, and the real test result (the PostTaskExec hook reminds you).
- Copy spike-verified code from design.md as is. Sketches may need small compile fixes, but keep their behavior.
- Do not add dependencies, features or files that a task does not ask for (tech.md, YAGNI). Mark any deliberate limit `shortcut: <limit>, <when to upgrade>`.

Target dates (internal, deadline Dec 4 2:00 PM PST):
- Kernel (tasks 1-7) by Nov 21.
- Both apps in fixture mode (tasks 8-9) by Nov 24.
- Live mode + UI (tasks 10-12) by Nov 27.
- Hardening + package (tasks 13-16) by Dec 1.
- Submit by Dec 3.

---

- [x] 1. Workspace scaffold
  - Create the root `Cargo.toml` (design §3), `wraithkit/` lib crate, and `apps/repo-wraith/` and `apps/research-wraith/` bin crates with an empty `main` that calls `wraithkit::cli::main::<...>()` once it exists. Add `.gitignore` (`/target`, `/.wraith`, `/evidence`), the `LICENSE` file (MIT unless the owner says otherwise) and `git init`.
  - Add `lib.rs` helpers: `sha256_hex`, `canonical`, `now_ms`, `args` and the limit constants.
  - Tests: `canonical_sorts_keys`, `sha_known_vector` (the abc vector).
  - Done when: `cargo test --workspace` passes, `git check-ignore -q .kiro` exits 1, and `git ls-files evidence` prints nothing.
  - _Requirements: 1.1, 12.1, 12.3_

- [x] 2. Taint barrier
  - Write `taint.rs` exactly as in design §5.
  - Add the doctests. Then prove the `compile_fail` tests fail for the right reason: temporarily make the field and `release` public, confirm the doctests now fail, revert, and record this in BUILD_LOG.
  - Tests: `gate_refuses_stale`, `gate_releases_matching`.
  - _Requirements: 3.1, 3.3_

- [x] 3. Ledger
  - Write `ledger.rs` (design §6): enums, `Line`, `line_hash`, `Ledger::{open, head, append, put_blob, blob}`, `read_blob`.
  - Tests: `ledger_chain_links`, `ledger_survives_torn_tail`, `ledger_rejects_corrupt_middle`, `ledger_is_exclusive`, `blob_roundtrip`, `blob_rejects_bad_name`, `event_serde_roundtrip` (one of each `Event` variant).
  - Use `std::env::temp_dir()` plus a unique folder per test, and remove it at the end. Tests run in parallel, so names must not collide.
  - _Requirements: 2.1, 2.2, 2.3, 2.4_

- [x] 4. App trait and evidence sink
  - Write `app.rs` (design §7): `Kind`, `ToolSpec`, `Preview`, `Demo`, `App`, `EvidenceSink::add` with the agent-phrase flag, and `builtin_tools()`.
  - Tests: `evidence_ids_sequential`, `flags_agent_addressed_text` (one positive, one negative).
  - _Requirements: 6.1, 6.3_

- [x] 5. Model adapter
  - Write `model.rs` (design §8): `Model::{fixture, live_from_env, label, next}`, `check_base_url`, `tool_calls`.
  - Tests:
    - `parses_openai_tool_calls`.
    - `live_request_shape`: factor the body-building into `fn request_body(model, messages, tools) -> Value` and test that function.
    - `model_base_url_rules`: https ok; http://localhost ok; http://example.com refused; garbage refused.
    - `fixture_script_end_is_error`.
  - Never print or format the key; the label shows only model and base.
  - _Requirements: 8.1, 8.3, 8.5_

- [x] 6. Session: fold, step, decide, resume
  - Write `session.rs` (design §9). In this order:
    1. `fold` and `describe`, with tests over hand-built event lists.
    2. `proposal_digest`, `quote_found`, `check_claim`.
    3. `Session::start` and `step`.
    4. `decide`.
    5. `resume`.
  - Add a test-only app `Counter` with one probe (`peek`, registers evidence "hello world text") and one effect (`bump {n}`, increments an `AtomicUsize`, input `{"counter": <current value>}`). Drive it with a fixture script.
  - Tests:
    - `effect_waits_for_approval`
    - `deny_never_executes`
    - `decide_rejects_wrong_seen_digest` (nothing appended)
    - `stale_input_blocks_effect` (change the counter between proposal and approval)
    - `double_decide_executes_once`
    - `resume_restores_pending`
    - `crash_mid_effect_is_outcome_unknown` (hand-append `EffectStarted`, resume, assert counter unchanged and status `needs_review`)
    - `effect_events_bracket_apply`
    - `claim_with_missing_quote_is_unsupported`
    - `unknown_tool_is_error`
    - `extra_field_is_error`
    - `budget_stops_run`
    - `tool_result_truncated`
    - `blocked_probe_records_blocked`
    - `digest_changes_with_any_input`
    - `quotes`
  - Done when: every Counter test reports the exact effect count it expects.
  - _Requirements: 3.2, 4.1, 4.3, 4.4, 4.5, 4.6, 5.1, 5.2, 5.3, 6.2, 8.5, 8.6_

- [x] 7. Verifier
  - Write `verify.rs` (design §10).
  - Tests:
    - `verify_clean_run_passes` (a Counter run)
    - `verify_detects_tamper`: for each line, change one character inside a string value (re-parse must still succeed), and verify must fail and name that seq; also corrupt one blob.
    - `verify_flags_unauthorized_effect` (hand-written ledger with `EffectStarted` and no approval, re-chained so only `authorized` fails)
    - `verify_while_open`
    - `key_never_persisted`
  - _Requirements: 7.1, 7.2, 7.3, 8.4_

- [x] 8. Repo Wraith (fixture mode, headless)
  - Implement the app (design §13): `resolve_in`, `is_secret_name`, the four tools, the prompt, the fixtures, `demo()` with `include_str!` for every fixture file, and `script.json`.
  - Write the fixture script by hand. Each `tool_calls[i].function.arguments` must be a JSON **string**. Validate the file with a test that parses it and checks every tool name exists.
  - Tests:
    - `paths`
    - `refuses_secret_files`
    - `refuses_large_and_binary`
    - `symlink_escape_refused` (skip with an `eprintln!` reason if symlink creation fails on Windows)
    - `edit_requires_unique_match`
    - `edit_applies_after_approval`
    - `ci_edit_warns`
    - `tools_have_no_network_or_shell`
    - `stale_input_blocks_effect` (edit money.py between proposal and approve)
    - `demo_runs_offline`: drive `Session` with the demo. Deny P2, approve P1, then assert `money.py` contains `round(amount * 100)`, `ci.yml` is unchanged, the `.env` read was blocked, and `verify` passes.
  - _Requirements: 1.2, 1.3, 9.1, 9.2, 9.3, 9.4, 4.4_

- [x] 9. Research Wraith (fixture mode, headless)
  - Implement the app (design §14): `check_url`, `is_public`, `html_to_text`, the allowlist from the target, fixture fetch through the same checks, `save_brief`, the prompt, the fixtures and `script.json`.
  - Tests:
    - `urls`, `ips`, `html` (copy from spike)
    - `offlist_host_refused`
    - `fixture_fetch_uses_same_checks` (`http://` and IP URLs refused even if listed in index.json)
    - `allowlist_from_target_only`
    - `brief_name_rules`
    - `brief_stale_if_created_meanwhile`
    - `blocked_fetch_visible`
    - `demo_runs_offline`: approve the brief and assert the file exists, the deadline claim is conflicted with 2 found cites, the wrong-quote claim is unsupported, evil.example is blocked, and `verify` passes.
  - `redirect_rechecked`: put the per-hop logic in a pure `fn next_hop(current, status, location) -> Result<Option<String>, String>` and unit-test that function (no network).
  - _Requirements: 1.3, 10.1, 10.2, 10.3, 10.4, 10.5_

- [x] 10. CLI and live mode
  - Status 2026-11-16: done. The manual smoke test ran once per app on Gemini (`gemini-flash-lite-latest`, Google's OpenAI-compatible endpoint) and is recorded in BUILD_LOG. Both runs finished and `verify` passed. Other providers are untested.
  - Write `cli.rs` (design §12). Wire `main` for both apps.
  - `run` requires `--live`. Print and record the egress disclosure (R8.2).
  - Tests: `live_requires_flag`, plus arg parsing for each subcommand.
  - Manual smoke test, recorded in BUILD_LOG with endpoint, model and date (never the key): one live `repo-wraith run` on the demo repo copy, and one `research-wraith run` with a 2-line `sources.txt` of public pages you are allowed to fetch. Note exactly what happened, including failures. If the endpoint rejects returned extra message fields, add the strip described in design §8.
  - _Requirements: 8.1, 8.2, 8.4_

- [x] 11. Server
  - Write `server.rs` (design §11): token, guards, routes, security headers, body cap, worker thread, `--open`.
  - Tests:
    - `server_guards`
    - `post_requires_json`: start on port 0 in a thread and send a raw HTTP request over `TcpStream`. Wrong Host gives 421, no token gives 404, text/plain POST gives 415, a good decide gives 200, and a repeated decide gives 409.
    - `state_view_has_card_fields`
    - `state_view_has_mode`
  - _Requirements: 11.1, 11.3, 4.2, 4.6_

- [ ] 12. UI
  - Status 2026-11-16: UI built and script-checked. OPEN: the manual checklist (keyboard, screen reader, true 360 px, reduced motion). Contrast is now measured: 18 pairs, all pass WCAG AA (lowest 5.05:1). The headless 360 px capture was invalid (Edge minimum width is 492 px).
  - Write `ui/index.html` (design §11), all `textContent`, no external assets.
  - Walk through both demos by keyboard only.
  - Manual checklist, recorded in BUILD_LOG with what passed, what failed and what was not checked:
    - Tab order and visible focus.
    - Narrator or NVDA reads the cards and the status changes.
    - 360 px width.
    - Reduced motion.
    - Contrast of each text color, checked with a contrast tool.
  - _Requirements: 4.2, 11.2, 11.3_

- [ ] 13. Hardening pass
  - Status 2026-11-16: clippy, fmt and the unwrap/expect review are done, and the demo ran on Windows. The full test suite also ran green on Ubuntu and macOS CI. OPEN: running the demo by hand on Linux.
  - `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.
  - Search for `unwrap(`/`expect(` outside tests and justify or remove each one.
  - Re-read tech.md's security rules and tick each against the code.
  - Run the full demo twice on Windows, once on Linux (WSL is fine).
  - _Requirements: all_

- [ ] 14. CI and release
  - Status 2026-11-16: rehearsed in a private test repo. ci.yml was green on Ubuntu, Windows and macOS (run 43783524001). A `v0.1.0-test` tag ran release.yml: all four builds passed and 8 zips were attached. The Windows zips were downloaded and both demos ran, with `verify` exit 0. OPEN: the real `v0.1.0` tag on the real repo, and a download test on a machine without Rust (this one has it).
  - Add `.github/workflows/ci.yml` and `release.yml` (design §17). Push to a **private** repo first, confirm green on all three OSes, tag `v0.1.0`, and download the Windows zip on a machine without Rust and run `demo`.
  - Making the repo public is a separate owner decision.
  - _Requirements: 12.1, 12.2_

- [x] 15. Review checklist (lean proof)
  - Each app contains only the App impl, prompt and fixtures (R1.2). Grep the apps for ledger, server and model code.
  - Measure line counts with `tokei` or a `git ls-files | xargs wc -l` equivalent. Record the numbers in BUILD_LOG and the README.
  - _Requirements: 1.2_

- [ ] 16. Package for submission (owner approves before anything is published)
  - Status 2026-11-16: README draft only. OPEN: GIF, Devpost text, blog post, social post and the video. The owner publishes.
  - Rewrite README.md as the product README:
    - what it is, with a 20-second GIF or screenshot
    - download and `demo` steps
    - build from source
    - live mode env vars and the egress note
    - how the trust layer works (the three ideas from product.md)
    - known limits (design §14 exfiltration note, unsigned ledger, heuristic flags, fixture vs live)
    - prior work: CaMeL, FIDES, lilith-zero, agentplane, the IETF receipt drafts
    - disclosure of the pre-existing research snapshot (2026-11-15, outside Kiro) and the third-party 1-SEC material that is not included
    - license
  - Draft the Devpost text: features, how Kiro was used (specs, steering, hooks; cite BUILD_LOG), category Skeleton Crew, and bonuses Social Blitz + Blog.
  - Draft the blog post. It must say it was written for the Kiroween 2026 hackathon.
  - Draft the social post. It must start with `#KiroweenHackathon26`, and name a favorite Kiro feature and how it changed the approach.
  - Record the video (design §18).
  - The owner reviews, publishes and submits.
  - _Requirements: 12.1, 12.3, 12.4_

---

## Handoff back to the reviewer (Opus) after task 13
Provide:
- the commit hash
- `cargo test --workspace` output
- the `verify` output for both demos
- line counts
- the BUILD_LOG entries
- any test you skipped, and why

The review pass runs the evaluation plan in design §19 and fixes what fails.
