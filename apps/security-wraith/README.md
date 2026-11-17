# Security Wraith

**Investigate the incident. Approve the exact response.** The headline WraithKit app reuses the shared Rust evidence, claims, approval and replay kernel.

## Run the offline demo

```sh
cargo run -p security-wraith -- demo
```

Open the loopback URL printed by the kernel. The model transcript and security observations are **fixtures**, not a live model or a running 1-SEC detector.

1. Inspect a synthetic event-queue receipt, correlated alerts, the scoped release bot, conflicting release-window context, and hostile log text.
2. Exact evidence quotes support the investigation. A deliberately unsupported shell-tool request is blocked. No shell tool exists.
3. Approve or deny **P1**: write `response-plan.json` in the fresh demo directory. Only this local simulation changes from `running` to `paused`.
4. After a state re-read, **P2** proposes the separate reversal. It requires another exact approval. Approving returns the local simulation to `running`.
5. Inspect the decision receipts and use `security-wraith verify <run-dir>` to check the evidence and ledger offline.

Denying P1 leaves the output absent. Its scripted rollback is then refused as stale rather than executing without a pause. Denying P2 leaves only the approved local simulation paused. The final summary deliberately does not claim both actions ran. No real agent, process, credential, firewall or network changes in any branch.

## Check a local sanitized export without a model

```sh
cargo run -p security-wraith -- check ./my-incident
```

The directory contains `incident.json`. `fixtures/incident.json` is the complete envelope example. For your own evidence, set `provenance` to `user_supplied_export`, remove sensitive data, and explicitly set `sanitized: true`. This flag and all provenance labels are **user assertions, not authentication or a guarantee of redaction**. Keep input and run directories private: raw observations and stored evidence can contain sensitive data.

The check is read-only and offline. It validates bounded structure, known severity/status values, unique IDs, event references, the explicit one-asset association, and any existing local plan. It does not run detectors, verify source authenticity, establish compromise, or independently verify a supplied asset mapping.

This is a **bounded sanitized subset/envelope**, not a universal direct importer of every 1-SEC API response. Export events and alerts, include their correlation reference, and supply asset/business context. Remove unsupported fields before importing; in particular raw binary `raw_data` and sensor credential/provenance objects are deliberately not accepted. Unknown fields fail closed. Preserve only the metadata needed to investigate. Empty/missing alerts mean there is insufficient evidence for this incident workflow, not that the system is safe.

Limits: 64 KiB per input file; 1–32 events, 1–16 alerts, at most 32 queue receipts and 16 context notes; each evidence section must fit 7,500 bytes. Only fixed `incident.json` and `response-plan.json` names are read. Linked, non-UTF-8, oversized and malformed files are refused. Use a local directory that no other process is changing concurrently.

## Import a real 1-SEC local export

First create the ignored `.wraith` directory if it does not already exist. Keep the reviewed source export outside the checkout:

```sh
security-wraith import-1sec ../sanitized-export .wraith/imported-incident
security-wraith check .wraith/imported-incident
```

This is an implemented **file handoff**, not an authenticated API client or live event subscription. The source directory has three fixed UTF-8 JSON files (at most 64 KiB each):

- `events.json`: an array of 1-SEC `SecurityEvent` objects, including every event referenced by the included alerts. Obtain these from your event export/archive; the alerts endpoint alone is not sufficient.
- `alerts.json`: the actual alerts API shape, `{"alerts":[...],"total":N}`. `total` must equal the included count.
- `mapping.json`: `sanitized:true`, `incident_id`, `selected_alert_id`, `asset` (`id`, `kind`, `owner`, `criticality`), `source_note` and `context`. See the [captured synthetic export example](fixtures/1sec-export/README.md). The asset association is explicitly your assertion; the selected alert need not be a cross-module correlation.

Redact the source files first. The bridge retains only allowlisted event/alert fields; it drops opaque details, raw data, sensor objects and original alert metadata. This deliberately loses some context, including details-only correlation references. It **does not scrub secrets from retained summaries, descriptions, usernames/URLs within text or other strings**. Inspect the mapped result before optional model use. Empty event source receives an explicitly unverified export-origin label. The bridge does not fabricate HTTP queue receipts.

All structural, event-reference and exact evidence-payload size checks run before output creation. The output must be a new directory under an existing parent; existing files/directories are refused. Only `incident.json` is created. Input leaf symlinks/reparse points are rejected; use trusted single-writer directories because filesystem metadata/open races are not an OS sandbox. On Unix the new directory/file use 0700/0600; on Windows they inherit the parent ACL. Keep exports outside the repository or under ignored `.wraith/` paths.

Imports always use `user_supplied_export`. A later response plan still requires the normal exact approval and stays `not_executed`. The import command is not registered as a model tool. It does not request a model, connect to 1-SEC, send alerts or execute a mitigation.

### CI use

The dedicated [handoff workflow](../../.github/workflows/1sec-handoff.yml) runs import/check against captured **synthetic** upstream-module output on Windows and Linux, with a read-only repository token, no persisted checkout credential, no model/API secrets, no artifact uploads and no deployment. Its [CLI checker](../../.github/scripts/check-1sec-handoff.py) also tests rejected inputs, opaque-field filtering, unchanged sources and overwrite/link refusal. The ordinary workspace CI continues to test the approval/session boundary on three OSes.

You can run `check` in your own trusted CI after preparing sanitized exports. A green result means this bounded handoff is structurally usable, **not** that the incident is safe, mitigated or false-positive. This workflow intentionally does not fetch production telemetry or post evidence to pull requests. Real data access would require separately scoped credentials, a reviewed redaction/export policy and explicit authorization.

## Optional model-assisted investigation

```sh
# Configure WRAITH_MODEL, WRAITH_BASE_URL and, if needed, WRAITH_API_KEY privately.
cargo run -p security-wraith -- run ./my-incident "Investigate this sanitized incident and propose a narrow response plan" --live
```

`--live` selects the existing WraithKit **model adapter only**. It explicitly sends task text and tool-result evidence to the configured model endpoint. It does not start or connect to 1-SEC. Without this explicit command the demo/check make no model requests. Never put credentials into a bundle or task.

Approved plans for `user_supplied_export` always retain `simulated_state: "not_executed"`. They are local recommendations, not operational containment. `action`, `reason`, `rollback_action`, the scoped asset/incident, revision and bundle hash remain in the JSON artifact for an operator to review. Replacing or reversing that plan requires another approval. There is no actuator, automatic external call, live 1-SEC connector, process runner or generic file-write tool.

## Exact-action boundary

The kernel binds approval to the tool, exact arguments, all observed evidence IDs, hashes of the whole incident and current plan, and the resolved output destination. Changing evidence, context, state or destination invalidates the card. The app checks expected hashes/revision again inside `apply`. A `Clean<Value>` from the kernel is required to reach that effect. Logs and alerts are data, not authority. This is a trusted local app boundary, not a hostile-code sandbox. A concurrent writer can still race the final read/rename; do not use this demo as a multi-user enforcement service.

The hash-chained ledger is tamper-evident relative to a retained head hash, not signed or tamper-proof. Quote presence proves the text was in the snapshot, not that a security claim is true.

## Relationship to 1-SEC

1-SEC detects/correlates supplied security telemetry; Wraith investigates the resulting evidence and proposes a reviewable response. This app ships independently authored JSON examples and Rust code, **not the 1-SEC engine or source**. No live integration is claimed.

The upstream event/alert field shapes were reviewed at [`simpleending/1-sec` revision `bf47bc6`](https://github.com/simpleending/1-sec/tree/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef). See [`SecurityEvent`](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/internal/core/event.go), [`Alert`](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/internal/core/alert.go), and the [API server](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/internal/api/server.go). `POST /api/v1/events` returning **202 `{status:"accepted",event_id:...}`** acknowledges asynchronous submission. It is not an allow/block verdict and does not guarantee any alert will follow. Integration and enforcement need their own connected implementation and upstream licensing review.

## Tests

```sh
cargo test -p security-wraith
```

Tests exercise the real shared Session: exact/double/stale decisions, separate rollback, denial, resume, all-input hashing, untrusted instructions, unsupported tools/targets, malformed and oversized exports, local-only imported plans, symlink refusal when supported by the OS, and offline ledger verification. The fixture and demo do not establish detection accuracy or live containment effectiveness.
