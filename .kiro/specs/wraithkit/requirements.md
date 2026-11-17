# WraithKit requirements

Format: EARS. Each acceptance criterion names the test that proves it. A box gets checked only when that test exists and passes on CI. "Kernel" means the `wraithkit` crate; "app" means `repo-wraith` or `research-wraith`.

Status 2026-11-16: the kernel and both apps are implemented. The ticked boxes (32 of 47) have a named test that passes on CI: run 43783524001 in the private test repo, green on Ubuntu, Windows and macOS. R8.1 is also ticked: its live request shape test passes, and the adapter ran once per app on Gemini (see BUILD_LOG). Unticked on purpose: 8.6 and 10.1 (their live parts, the 90-second timeout, redirects and the size cap, have no test), 4.2 and 11.3 (partly manual), 10.2 (its named test does not exist, and the demo test covers it), and the manual or release items (1.1, 1.2, 3.1, 3.2, 9.3, 11.2, 12.x). Open: the manual UI checklist (R11.2) and any measured evaluation of model behavior.

---

## R1. One kernel, two apps (Skeleton Crew)
**User story:** As a judge, I want to see two different apps built from one small template, so that the template's versatility is obvious.

- [ ] 1.1 THE SYSTEM SHALL consist of one library crate `wraithkit/` and two binary crates in separate folders `apps/repo-wraith/` and `apps/research-wraith/`, each depending on `wraithkit`. *Test:* CI builds the workspace; `cargo tree -p repo-wraith` shows `wraithkit`.
- [ ] 1.2 Each app SHALL contain only an `App` trait implementation, its prompt and fixtures. It SHALL NOT implement its own ledger, approval, model, server or UI. *Test:* review checklist in task 15; `grep -r "tiny_http\|ureq::Agent::config_builder\|ledger.jsonl" apps/` finds only the Research fetch agent.
- [x] 1.3 WHEN an app is started with `demo`, THE SYSTEM SHALL run fully offline using a scripted fixture model and bundled files, and SHALL show the label `FIXTURE: scripted, not a live model` in the CLI output, the UI header and the `RunStarted` event. *Test:* `demo_runs_offline` (each app) runs with no network and checks the label.

## R2. Append-only, hash-chained ledger
**User story:** As a user, I want a durable record of everything the agent saw and did, so that I can resume and audit a run.

- [x] 2.1 WHEN any event occurs, THE SYSTEM SHALL append one JSON line `{seq, prev, at_ms, event, hash}`, where `hash = sha256(canonical({seq, prev, at_ms, event}))` and `prev` is the previous line's hash (empty string for seq 0), and SHALL `sync_data` before continuing. *Test:* `ledger_chain_links`.
- [x] 2.2 WHEN a run is opened and the final line has no trailing newline, THE SYSTEM SHALL drop that torn line. IF any complete line fails to parse, THEN THE SYSTEM SHALL refuse to open the run and name the line number. *Test:* `ledger_survives_torn_tail` (spike-verified), `ledger_rejects_corrupt_middle`.
- [x] 2.3 IF another process already has the run open, THEN THE SYSTEM SHALL refuse with "another WraithKit process has this run open". *Test:* `ledger_is_exclusive` (spike-verified).
- [x] 2.4 THE SYSTEM SHALL store evidence text as content-addressed blobs `blobs/<sha256>.txt` and SHALL only read blobs by a 64-hex-char name. *Test:* `blob_roundtrip`, `blob_rejects_bad_name`.

## R3. Sealed effects (taint barrier)
**User story:** As a user, I want a guarantee that nothing the model produced can change anything without my approval, so that a prompt injection cannot act on my behalf.

- [ ] 3.1 THE SYSTEM SHALL pass arguments to effect tools only as `Clean<Value>`. `Clean` SHALL have a private field and `taint::release` SHALL be `pub(crate)`, so app crates cannot create a `Clean`. *Test:* two `compile_fail` doctests (construct `Clean`, call `release`), each paired with a passing doctest.
- [ ] 3.2 THE SYSTEM SHALL treat every model-produced tool argument as tainted, including arguments to read-only tools. *Test:* code review in task 15; `probe` receives `&Value`, never `Clean`.
- [x] 3.3 `release` SHALL refuse unless the approved digest equals the digest recomputed at release time. *Test:* `gate_refuses_stale` (spike-verified).

## R4. Proposals and approvals bound to data history
**User story:** As a user, I want to see exactly what an action will do and what influenced it, and my approval must not apply to anything else.

- [x] 4.1 WHEN the model calls an effect tool, THE SYSTEM SHALL NOT execute it. It SHALL ask the app for a `Preview` (summary, detail, warnings, input hashes as they are now), compute `digest = sha256(canonical({tool, args, inputs, influenced_by}))` where `influenced_by` is every evidence ID acquired so far in the run, append `Proposed`, and set the run to `waiting`. *Test:* `effect_waits_for_approval`, `digest_changes_with_any_input` (spike-verified).
- [ ] 4.2 THE SYSTEM SHALL show in the approval card: summary, detail (diff or file preview), warnings, each input with its hash prefix, each influencing evidence item with its source and flags, and the first 12 hex chars of the digest. *Test:* manual UI check in task 12; `state_view_has_card_fields`.
- [x] 4.3 WHEN the user approves, the request SHALL carry the digest the user was shown. IF it differs from the proposal's digest, THEN THE SYSTEM SHALL reject the request and write nothing. *Test:* `decide_rejects_wrong_seen_digest`.
- [x] 4.4 IF the digest recomputed at approval time differs from the proposal digest (for example the file was edited), THEN THE SYSTEM SHALL append `Decided {approve: false, reason: "stale: ..."}`, tell the model, and not execute. *Test:* `stale_input_blocks_effect` (each app).
- [x] 4.5 WHEN the user denies, THE SYSTEM SHALL append `Decided {approve: false}`, return "The user denied this action. Do not retry it." to the model, and never execute. *Test:* `deny_never_executes` (each app, with an effect counter or file hash check).
- [x] 4.6 THE SYSTEM SHALL execute each proposal at most once, including on a double click, two browser tabs or a repeated POST. *Test:* `double_decide_executes_once`.

## R5. Restart safety
**User story:** As a user, I want to close the app and come back later without losing or repeating anything.

- [x] 5.1 WHEN a run is resumed, THE SYSTEM SHALL rebuild all state only by folding the ledger. Pending proposals SHALL reappear and remain decidable. *Test:* `resume_restores_pending` (drop the session after `Proposed`, reopen, approve, effect runs once).
- [x] 5.2 IF the ledger has `EffectStarted` without a matching `EffectDone`, THEN on resume THE SYSTEM SHALL tell the model "Outcome unknown: the process stopped during this action. It was not retried.", append `RunFinished {status: needs_review}`, and never re-execute it. *Test:* `crash_mid_effect_is_outcome_unknown`.
- [x] 5.3 THE SYSTEM SHALL write `EffectStarted` (synced) before calling `App::apply`, and `EffectDone` after. *Test:* covered by 5.2 plus `effect_events_bracket_apply`.

## R6. Evidence and grounded claims
**User story:** As a reader, I want every claim tied to an exact quote in a stored source, and unsupported claims labeled as such.

- [x] 6.1 WHEN an app probe registers evidence, THE SYSTEM SHALL write the blob, append `Evidence {id, source, sha256, bytes, flags}` and return the ID (`E1`, `E2`, ...). *Test:* `evidence_ids_sequential`.
- [x] 6.2 WHEN the model calls `claim`, THE SYSTEM SHALL check each cite's quote against the cited blob after collapsing whitespace, requiring at least 8 characters. IF an evidence ID is unknown or a quote is not found, THEN that cite SHALL be recorded `found: false` and the claim's status SHALL become `unsupported`. *Test:* `quotes` (spike-verified), `claim_with_missing_quote_is_unsupported`.
- [x] 6.3 WHEN evidence text contains phrases addressed to AI agents (list in design.md), THE SYSTEM SHALL flag it `addresses_ai_agents`, and the approval card SHALL show the flag. This is a heuristic, and the UI SHALL say that a missing flag does not mean the text is safe. *Test:* `flags_agent_addressed_text`.

## R7. Offline verification
**User story:** As a judge, I want to check a run's record myself, offline.

- [x] 7.1 WHEN `<app> verify <run-dir>` runs, THE SYSTEM SHALL check: chain links and hashes; blob hashes; every cite's `found` value recomputes; every `Proposed` digest recomputes; every `EffectStarted` has an earlier `Decided {approve: true}` with the same digest; at most one `EffectStarted` per proposal; and, if `WRAITH_API_KEY` is set, that the key does not appear in the ledger or blobs. It SHALL print one line per check plus the head hash, and exit non-zero if any check fails. *Test:* `verify_clean_run_passes` (each app's demo).
- [x] 7.2 WHEN any byte of a ledger line or blob is changed, `verify` SHALL fail and name the line or blob. *Test:* `verify_detects_tamper` (flip a byte in each event kind).
- [x] 7.3 `verify` SHALL NOT take the ledger lock, so it works while a session is open. *Test:* `verify_while_open`.

## R8. Model adapter
**User story:** As a user, I want to use my own model endpoint, and to know exactly when data leaves my machine.

- [x] 8.1 THE SYSTEM SHALL support exactly one live adapter: OpenAI-compatible `POST {base}/chat/completions` with `tools`. Configuration comes from `WRAITH_BASE_URL` (default `https://api.openai.com/v1`), `WRAITH_MODEL` (required) and `WRAITH_API_KEY` (optional for local servers). *Test:* `live_request_shape` (no network: builds the request JSON); one manual live smoke test in task 10.
- [x] 8.2 WHEN `--live` is used, THE SYSTEM SHALL print and record "LIVE: task text and tool results will be sent to <base>" before the first request. Live mode SHALL require `--live`, and `demo` SHALL never use it. *Test:* `live_requires_flag`.
- [x] 8.3 THE SYSTEM SHALL require HTTPS for the model base URL unless the host is `localhost` or `127.0.0.1`. *Test:* `model_base_url_rules`.
- [x] 8.4 THE SYSTEM SHALL never write the API key to the ledger, blobs, UI, errors or logs. *Test:* `key_never_persisted` (run fixture with the env var set; scan the run dir).
- [x] 8.5 IF tool arguments are not valid JSON, have unknown fields, or name an unknown tool, THEN THE SYSTEM SHALL return an error result to the model and run nothing. *Test:* `parses_openai_tool_calls` (spike-verified), `unknown_tool_is_error`, `extra_field_is_error`.
- [ ] 8.6 THE SYSTEM SHALL stop the run as `failed` with a stated reason when it exceeds 16 model turns or 40 tool calls, or when a model call exceeds 90 s. Tool results sent to the model SHALL be capped at 8,000 characters (the blob keeps the full text). *Test:* `budget_stops_run`, `tool_result_truncated`.

## R9. Repo Wraith
**User story:** As a developer, I want an agent to find a bug in my repo and propose a fix that I approve, without it reading secrets or escaping the folder.

- [x] 9.1 Probes `list_files`, `read_file`, `search` SHALL be confined to the canonicalized repo root. They SHALL refuse: empty, absolute, drive-prefixed, `..`, `.`, colon-containing (Windows alternate streams) and `.git` paths, symlink or junction escapes, secret-looking files (list in design.md), files over 256 KB and non-UTF-8 files. *Test:* `paths` (spike-verified) plus `refuses_secret_files`, `refuses_large_and_binary`, `symlink_escape_refused` (skipped with a printed reason where the OS forbids creating symlinks).
- [x] 9.2 The only effect, `edit_file {path, find, replace}`, SHALL require `find` to match exactly once. Its input is the file's current SHA-256. It writes via a temp file plus rename in the same folder, and SHALL NOT create new files. *Test:* `edit_requires_unique_match`, `edit_applies_after_approval`.
- [ ] 9.3 Repo Wraith SHALL have no network or shell tool. *Test:* `tools()` list assertion.
- [x] 9.4 The preview SHALL warn when the path is under `.github/` or its name contains `workflow`, `Makefile`, `build.rs` or `package.json`. *Test:* `ci_edit_warns`.

## R10. Research Wraith
**User story:** As a builder, I want a cited brief that keeps conflicts between sources visible and never fetches places I did not allow.

- [ ] 10.1 `fetch {url}` SHALL accept only `https`, port 443, and a host name (not an IP literal) on the run's allowlist. All resolved addresses SHALL be public. It SHALL follow at most 3 redirects, re-checking each hop, and read at most 2 MB. Only `text/*`, HTML and JSON are accepted, and HTML is converted to text. *Test:* `urls`, `ips`, `html` (spike-verified), `redirect_rechecked`, `offlist_host_refused`.
- [ ] 10.2 WHEN a fetch is refused or fails, THE SYSTEM SHALL return the reason to the model and show it in the timeline as a blocked fetch. *Test:* `blocked_fetch_visible` (demo includes an injected `evil.example` fetch).
- [x] 10.3 The only effect, `save_brief {name, markdown}`, SHALL write only `briefs/<name>.md` next to the target, where `name` matches `[a-z0-9-]{1,40}`. Its input is the existing file's hash or `absent`. *Test:* `brief_name_rules`, `brief_stale_if_created_meanwhile`.
- [x] 10.4 In fixture mode, `fetch` SHALL run the same URL and allowlist checks, then read the bundled file mapped from that URL instead of using the network. *Test:* `fixture_fetch_uses_same_checks`.
- [x] 10.5 The allowlist SHALL be the hosts of the URLs in the target sources file (live) or in `index.json` (fixture). The model cannot change it. *Test:* `allowlist_from_target_only`.

## R11. Local UI
**User story:** As a user, I want to see and decide everything in one clear, accessible page.

- [x] 11.1 THE SYSTEM SHALL bind `127.0.0.1` only, put a random token as the first path segment, reject a `Host` other than `127.0.0.1:<port>` or `localhost:<port>` with 421, and accept POST only with `Content-Type: application/json`. *Test:* `server_guards` (spike-verified), `post_requires_json`.
- [ ] 11.2 The UI SHALL work by keyboard alone, show a visible focus ring, announce status changes in an `aria-live` region, give every control a label, show status as text plus an icon (never color alone), respect `prefers-reduced-motion`, meet WCAG AA contrast, and work at 360 px wide. *Test:* manual checklist in task 12, recorded in BUILD_LOG with what was and was not checked.
- [ ] 11.3 The mode label (FIXTURE or LIVE + model) SHALL always be visible. *Test:* manual check; `state_view_has_mode`.

## R12. Packaging and Kiroween compliance
**User story:** As the entrant, I want the submission to meet every rule.

- [ ] 12.1 The public GitHub repository SHALL include an OSI license that GitHub detects, `/.kiro` at the root and not gitignored, both app folders, and a README with install and run steps. *Test:* CI step `git check-ignore -q .kiro && exit 1`; manual check of the repo page.
- [ ] 12.2 Release builds of both apps for Windows, macOS and Linux SHALL be attached to a GitHub Release, so judges need no Rust toolchain. *Test:* release workflow run, plus a manual download-and-`demo` check on Windows.
- [ ] 12.3 BUILD_LOG.md SHALL record Kiro work honestly, and the README SHALL disclose the pre-existing research snapshot. `evidence/` SHALL NOT be published. *Test:* manual; `git ls-files evidence` is empty.
- [ ] 12.4 The demo video SHALL be under 3 minutes and show only behavior the repository reproduces, with fixture and live footage labeled. *Test:* manual review against the storyboard in design.md.

---

## Open owner questions (block submission, not build)
1. License: MIT recommended (matches all dependencies). Confirm.
2. Personal eligibility, and the fact-gated bonuses (startup, student, Kiroween 2025 registration).
3. Whether a GitHub Releases link satisfies "URL to your functional application(s)". It is the common practice for CLI/desktop entries, but ask Devpost support to be sure.
4. Which live endpoint appears in the video, and confirmation that its terms allow this use.
