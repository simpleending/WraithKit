# WraithKit submission packet

Draft for owner review, 2026-11-17. This is not a completed hackathon submission. The public source is [simpleending/WraithKit](https://github.com/simpleending/WraithKit). The [landing page](https://wraithkit.vercel.app/) is a project introduction and browser-ledger demo, not a hosted Rust application. Insert the missing release, working-app demonstration and video links before submitting.

## Submission gates

Rechecked against the [official 2026 rules](https://kiroween.devpost.com/rules) and [event requirements](https://kiroween.devpost.com/) on 2026-11-17:

- Deadline: **December 4, 2026, 14:00 PST / 22:00 UTC**. Project work must be new within the October 30 to December 4 submission period.
- Submit a public, open-source **GitHub** repository with all required code, assets and instructions, root `.kiro/`, and separate folders for both Skeleton Crew applications.
- Provide free judge access to a functioning application, demo or test build through the end of judging. A marketing page alone is not a test build.
- Include a feature description, genuine Kiro-use write-up, category/bonus selections and a **public YouTube video under three minutes** showing the actual software. Use English or supply translations. Clear asset rights first.
- Owner must review eligibility, ownership and the entry terms. Do not infer student, startup, prior-participant or Kiro Crew eligibility.

Current decisions still needed:

- [x] Owner selected MIT for the hackathon on 2026-11-17; existing `LICENSE` and Cargo metadata verified.
- [ ] Owner confirms eligibility and project dates.
- [x] Public GitHub source is available at `simpleending/WraithKit`; initial snapshot `b84e3d6` passed the public three-OS and synthetic handoff CI runs linked in README. Downloadable releases remain pending.
- [ ] Current CI and downloadable builds match that commit. Test downloaded builds, not just the developer checkout.
- [ ] Working-app/test-build URL is inserted into the submission. Verify access while signed out.
- [ ] Record, review and publish the video; insert its YouTube URL.
- [ ] Owner reviews the Kiro narrative below against the original development record.
- [ ] Choose only substantiated bonus categories and provide their required material.
- [ ] Exclude private research snapshots, run data, credentials and unrelated files from the public package.

The owner-approved MIT file, shared crate and four application folders are committed in the private rehearsal repository; Security and Release are the designated main pair. Reviewed application commit `99a0c46675e92a954a5df9b5081fb88199ba42f5` passed Windows, Ubuntu and macOS hosted CI (run `44080690459`), and four local Windows archives passed extracted-binary smoke checks. This is private validation, not a public release or clean-machine installation test. Recheck the selected public source/build commit and judge access at publication.

WraithBridge adds an optional Release-to-Research local handoff. It transfers a typed, bounded report as untrusted evidence, never approval. The receiving app requires a fresh decision before saving its brief. See [BRIDGE.md](BRIDGE.md) for the real two-binary walkthrough and the current validation record in README. This later ChatGPT/Cortana-assisted addition is not claimed as historical Kiro work.

## Project description draft

**WraithKit: four examples, one approval boundary. Security and Release lead the demonstration.**

Agent apps read untrusted files and web pages, then propose changes. WraithKit is a small Rust foundation for making those changes inspectable before they happen. It supplies the model adapter, human approval cards, resumable event log, evidence storage, local browser UI and offline verifier.

The two main apps reuse that foundation for different jobs. Security Wraith investigates supplied incident evidence and proposes a scoped local response plan, retaining conflicting context and requiring a separate approval for reversal. Release Wraith checks actual local artifact bytes against a release plan and proposes an exact path correction. Repo Wraith and Research Wraith remain runnable bonus examples. Each app contributes its tools, prompt and fixtures rather than copying the kernel.

An effect proposal binds the tool, arguments, declared input state and evidence IDs into a digest. The user sees the preview; the kernel checks the digest again before releasing the effect. A sealed Rust type prevents an app crate from constructing an approved value through the public API. A hash-chained ledger keeps the decisions and outcomes inspectable after a restart.

The model demonstrations are deliberately scripted and labeled as fixtures. Security shows queue receipts versus detection evidence, conflicting incident context, an unsupported shell request, and separate approvals for a local pause/reversal simulation. Release shows a local contract FAIL, an approved plan repair, and PASS after actual artifact-byte hashing. Neither demonstrates live 1-SEC containment or a hosted CI rerun. Both also provide deterministic read-only check commands for user-supplied local exports. The original Repo/Research scenarios remain available as bonus examples.

This is a local, single-user developer toolkit, not an operating-system sandbox. App implementations are trusted. A quote match is not a truth check; a hash chain is not a signature. The README documents filesystem race, DNS rebinding and other limits. One recorded live smoke test per original Repo/Research app is not a broad evaluation; no real-provider run is claimed for Security or Release.

## Kiro-use narrative draft

The development record in [BUILD_LOG.md](BUILD_LOG.md) attributes the Rust implementation to Kiro IDE sessions on November 16, 2026. A ChatGPT-assisted research/specification snapshot preceded those sessions on November 15. That initial Python proposal was replaced, and no 1-SEC implementation was imported. Later ChatGPT/Cortana review and documentation work is disclosed separately rather than represented as Kiro work.

The Security/Release extensions added on November 17 were ChatGPT/Cortana-assisted work, not historical Kiro sessions. Do not describe those extensions as Kiro-generated without a separate genuine record. The original kernel is the reused Kiro-built foundation.

The original Kiro workflow used three layers. [Requirements](.kiro/specs/wraithkit/requirements.md) connect acceptance criteria to named checks. The [design](.kiro/specs/wraithkit/design.md) records the trust boundary, event model, rejected alternatives and deliberate limits. The [task list](.kiro/specs/wraithkit/tasks.md) breaks the work into 16 ordered steps with explicit completion checks. That structure made approval, stale-input and restart behavior reviewable before adding the two apps.

[Steering](.kiro/steering/) kept the stack small and synchronous, retained security rules, and prohibited unsupported product claims. The log records a throwaway Rust spike before implementation and later review passes that found concrete defects, added regressions and, for selected checks, removed fixes temporarily to confirm the tests failed.

Two [hook configurations](.kiro/hooks/) are included: Rust formatting after saves and a reminder to record finished tasks honestly. A local review of the genuine Kiro IDE build history confirmed reads of the requirements, design and task list, inclusion of the three steering documents, and 75 completed Format Rust on save hook invocations. That is evidence of use, not a measured time saving. Execution of the task-log hook was not independently verified. Private session transcripts and identifiers are not included in this public draft.

The most useful outcome was traceability: requirements, code, tests and recorded limitations can be followed together. The repository does not substantiate claims of Kiro Crew, Kiro Web/CLI, MCP, Kiro powers or property-based testing usage, so this write-up makes none.

### Evidence to show, not manufacture

- Open the real `.kiro/specs/wraithkit/` files and one named regression beside its implementation.
- Use the dated build log as the repository's development account, not as independent proof of every test result.
- If adding a Kiro screenshot, capture the genuine session or hook history, with private content removed. Never recreate an old session or backdate evidence.
- Keep current validation distinct from the historical CI rehearsal. Report the platform, commit, command and actual result for the final release.

## Judge run instructions

### Prerequisites

Use a desktop browser and Rust **1.96 or newer** with its normal build prerequisites. Run from the repository root. The fixture demos need no account or model key and make no live model requests. Cargo's first build may download dependencies; after caching them, Cargo's `--offline` flag prevents dependency downloads. Build time is not included in the demonstration runtime.

For the eventual release, provide the matching app binaries for the judge's platform. Replace each Cargo prefix with the executable: for example, `.\security-wraith.exe demo` on Windows or `./security-wraith demo` on macOS/Linux, and likewise for the other three apps. The recorded release rehearsal produced unsigned binaries, not an installer. Public download links are still pending; do not substitute private rehearsal links.

### 1. Security Wraith (main)

```sh
cargo run --locked -p security-wraith -- demo
```

Open the fresh printed local URL. The fixture label must remain visible: neither the model nor the security incident is live.

1. Inspect the queue receipt and quoted alerts. HTTP 202 is asynchronous acceptance, not a verdict or a guarantee that an alert follows.
2. Inspect the competing legitimate-release context and the hostile log instruction. No shell tool exists; the unsupported request is refused.
3. Read P1's exact local output and input hashes. **Approve P1** to write only `response-plan.json` in the disposable target. The local fixture state becomes paused; no real agent or system changes.
4. After the state re-read, inspect **P2**, the reversal. It requires another approval. Approve it and inspect revision 2 / running in the local simulation.
5. Verify the ledger. A separate fresh run can demonstrate denial or stale inputs without changing the source fixtures.

For practical non-demo use, `security-wraith check <bundle-directory>` validates a bounded sanitized export, offline and read-only. Optional `run ... --live` uses a model, not a live 1-SEC connector. Plans created for imported evidence remain `not_executed`. See [Security's exact format and limits](apps/security-wraith/README.md).

### 2. Release Wraith (main)

```sh
cargo run --locked -p release-wraith -- demo
```

1. Inspect the supplied historical CI failure/log, the release plan and the artifact inventory. These fixtures are not fetched CI.
2. Inspect the local contract FAIL: the plan points to a missing artifact, while another local artifact's bytes and SHA-256 are verified.
3. Read the whole proposed `release-plan.json` change and its dependency hashes. **Approve P1** to repair only its artifact path.
4. Inspect the actual local checker result: PASS for path/byte consistency. The historical CI conclusion remains failure; `ci_rerun` and `published` remain false. No build, test suite, archive-content validation, tag, upload or deployment occurs.
5. Verify the ledger. Denial leaves the original plan unchanged; changed inputs reject stale approval.

`release-wraith check <bundle-directory>` is independently useful without a model: exit 0/PASS for a matching local contract, 1/FAIL for the mismatch, 2/INVALID for invalid input. See [Release's exact format and limits](apps/release-wraith/README.md).

### Bonus example: Repo Wraith

```sh
cargo run --locked -p repo-wraith -- demo
```

Keep that terminal running. Open the **fresh URL printed by this process**, including its random path. The server binds only to `127.0.0.1`; this address is for the judge's own machine, not a public application link. The UI should say `FIXTURE: scripted, not a live model`.

The script advances automatically to the approval cards:

1. Find the blocked `.env` read in the timeline. It did not read a real secret.
2. Inspect P2, the CI-file edit. Its preview contains an unwanted command and a build/CI warning. Open the flagged README evidence to see the source text that influenced it.
3. Choose **Deny P2**. This matters: the toolkit does not automatically know that every proposed edit is wrong.
4. Inspect P1, the change from `int(amount * 100)` to `round(amount * 100)` in `invoice/money.py`, then choose **Approve P1**.
5. Wait for completion. The app edits a disposable copy of the synthetic repository, not WraithKit's source checkout. The scripted task does not execute the Python tests.
6. Choose **Verify ledger**, or use the terminal command below with the run-folder path printed by this same process.

```sh
cargo run --locked -p repo-wraith -- verify ".wraith/runs/REPLACE_WITH_PRINTED_RUN_FOLDER"
```

The placeholder is not a real path. The verifier should report passing integrity/authorization checks; with no `WRAITH_API_KEY` set, its secret-value scan is explicitly skipped, not evidence that every possible secret was checked. Retain the head hash separately if you want a reference for later tamper detection.

### Bonus example: Research Wraith

In a second terminal, from the same repository root:

```sh
cargo run --locked -p research-wraith -- demo
```

1. Open its own freshly printed URL and confirm the fixture label.
2. Inspect the conflicting deadline claim and both quotes. These are synthetic Hollow Hacks pages, not research into Kiroween's actual rules.
3. Inspect the unsupported team-size claim. Its quoted text is absent from the stored evidence. The kernel checks quote presence; it does not independently decide which deadline is true.
4. Find the blocked off-list fetch in the timeline.
5. Read the complete `save_brief` preview, then **Approve P1**. Wait for completion. The brief is saved under the disposable demo target's `briefs/` directory.
6. Choose **Verify ledger**, or verify the research run folder:

```sh
cargo run --locked -p research-wraith -- verify ".wraith/runs/REPLACE_WITH_PRINTED_RUN_FOLDER"
```

### Optional automated fixture check

With PowerShell 7 (`pwsh`) available, build all four binaries and run the same offline CLI/HTTP smoke check used by CI:

```sh
cargo build --workspace --locked
pwsh -File .github/scripts/smoke-demos.ps1 -BinDir target/debug
```

This checks all four fixture flows and their output/ledger behavior, with separate denial/stale-input cases for the main pair. It is not a live-model evaluation or a manual accessibility test.

### Optional restart and stale-approval checks

Use a **new** Repo Wraith demo. While P1 and P2 are pending, stop its terminal process with Ctrl+C, then resume the printed run folder:

```sh
cargo run --locked -p repo-wraith -- resume ".wraith/runs/REPLACE_WITH_PRINTED_RUN_FOLDER"
```

Open the new URL. The pending cards should return. A run cannot be opened twice at once; stop the original process before resuming.

For a separate stale check, start another fresh demo. Locate its disposable target using the `target` field in the first `run_started` event of that run's `ledger.jsonl`. Do not edit WraithKit's fixture source. Append a harmless comment to that target's `invoice/money.py` while P1 is pending, keeping the original `int(amount * 100)` line intact. Approve P1. The changed input hash should make P1 stale rather than applying its old preview. Deny P2. Start a new demo to repeat the successful approval path.

Live mode is optional and unnecessary for judging these mechanics. It requires explicit `--live` configuration and sends task/tool data to the chosen model endpoint. Do not add a model key to screenshots, the repository, a URL or a submission form. See [README live-mode instructions](README.md#live-mode).

## Demonstration script: target 2 minutes 45 seconds

Record real desktop footage. Build beforehand, prepare two fresh fixture runs, and keep the fixture labels readable. This is a shot plan, not a claim that a video has been recorded. Do not present cuts as an uninterrupted execution. Use no third-party music or unlicensed assets. Hide private tabs, local account names, live-run data and the session URL token in the final video.

| Time | Show | Narration |
|---|---|---|
| 0:00-0:15 | Five workspace crates; main and bonus folders | "Two main applications and two bonus examples reuse one small Rust foundation. The evidence, approval gate and verifier are shared, not copied." |
| 0:15-0:30 | Security terminal, fixture label and event receipt | "This is a functioning local app with a scripted model and synthetic observations. A 202 receipt means queued, not safe or detected." |
| 0:30-1:05 | Conflicting context, unsupported shell request, P1 preview and approval | "Logs cannot grant authority. I inspect a narrow response proposal and approve its exact local file change. This is a containment simulation, not a command to 1-SEC." |
| 1:05-1:20 | P2 reversal and verified ledger | "Reversal is a different action, with a fresh state read and a separate approval. The ledger records what actually ran." |
| 1:20-1:55 | Release evidence, FAIL and full P1 preview | "The second app diagnoses a release-plan mismatch. It hashes actual local artifact bytes. Approval repairs only the declared path, not the code or a hosted workflow." |
| 1:55-2:15 | Local PASS, historical failure/no-publication fields, verify | "The local contract now passes. CI was not rerun and nothing was published. These validators also accept user-supplied local exports without a model." |
| 2:15-2:35 | Genuine .kiro specs/steering and dated development record | "Kiro built the shared foundation with specs, steering and hooks. Later extensions and review work are disclosed separately. The record distinguishes actual runs from fixture demonstrations." |
| 2:35-2:45 | Main two folders and bonus Repo/Research | "Skeleton Crew is the reusable foundation. This is a local developer toolkit, not an operating-system sandbox or a claim that prompt injection is solved." |

Before uploading, time the final export, review every visible screen for private data, and rerun the exact release build used in the footage. Put actual repository, test-build and video URLs in the submission only after they work while signed out.
