# Optional Release-to-Research WraithBridge

This is a real, offline two-app handoff, not automatic cross-agent execution. Release Wraith validates actual local artifact bytes and exports a typed report. Research Wraith reads that report as untrusted evidence and proposes a deterministic, cited Markdown brief. A fresh exact approval is required before the brief is saved.

## Try it

Build cached dependencies with `cargo build --locked --offline -p release-wraith -p research-wraith`. The commands below use binaries on your PATH; Cargo users can prefix them with `cargo run --locked --offline -p <app> --`.

1. Run `release-wraith demo`. It uses synthetic CI input and a scripted model, but its local artifact checks and approved plan edit really execute. Approve P1 for a locally consistent plan or deny it to retain the FAIL result. Use the target folder shown in the local UI.
2. Run `release-wraith export-bridge <target-folder> <new-handoff.json>`. The output parent must already exist; output must not exist. The exporter reruns the check rather than trusting a stored status. This explicitly invoked command writes the new evidence file only.
3. Run `research-wraith bridge <new-handoff.json>`. Open its printed localhost URL. The ordinary Wraith UI shows the full imported envelope as evidence and a new proposal. The adapter is deterministic and offline, not a live model or a claim of independent analysis.
4. Before approval, no brief exists. Approve to save exactly the displayed content to `briefs/release-handoff.md` beside the handoff. Deny to write nothing. An existing brief is disclosed and bound to approval.
5. Use `research-wraith bridge-resume <research-run-folder>` to resume the same adapter/script, not the unrelated Hollow Hacks demo. `research-wraith verify <run-folder>` verifies either app's ledger offline.

The optional commands do not change ordinary Research demo/live-source behavior. `bridge` and `bridge-resume` refuse `--live` and expose no fetch, shell, workflow, publication or enforcement tools. Only `--port N` and `--open` are optional.

## What is and is not trusted

The versioned envelope must name `release-wraith` as producer, `research-wraith` as recipient and `release-contract-report` as type. Its strict receiver-owned payload, consistent source label, four input-document hashes and exact artifact-hash manifest are checked. The shared decoder rejects unsupported versions, unknown/duplicate fields, mismatched digests, invalid UTF-8, floats and oversized packets (7,500 bytes total). Paths in report text are inert data, never read or executed by Research.

A digest proves byte consistency only. It is **not a signature or proof that Release generated the packet**. A malicious author can forge a consistent packet. Fixture/local-export provenance is an assertion, not authentication or redaction. Research does not re-read the original artifact bytes or authenticate a CI provider. The source may have changed after export; regenerate the handoff to refresh it. All untrusted strings remain quoted JSON in the saved brief, not Markdown instructions/links. No approval, capability, URL allowlist or system instruction transfers.

Input bytes are pinned when the Research run opens. Changes require a new run and invalidate a pending approval; resume also checks the recorded proposal's exact input/output hashes. The full derived brief hash, input path/hash, and destination path/state are approval dependencies. Repeating a decision cannot apply twice. Importing the same artifact into a new run is allowed but requires another approval; this is not a global replay registry.

Only ordinary files/ancestors are accepted; symlinks, Windows reparse points and special files are refused. The existing atomic brief writer preserves the previous file on write failure. These checks are a local single-user application boundary, **not an OS sandbox or a filesystem transaction against malicious concurrent writers**. Do not share unsanitized source metadata. No remote service, build, test, CI rerun or release publication occurs.

## Checks

`cargo test --locked --offline -p release-wraith -p research-wraith` covers producer checks and fresh consumer approval, denial, changed bytes/re-signed packets, destination changes, distinct resume, duplicate decisions, exact preview, schema/digest/size/path failures, and quoted hostile text. Link/FIFO checks are platform-specific; unavailable link creation is reported as SKIP. Existing internet-dependent Research tests remain opt-in/ignored. The repository's separate two-binary bridge smoke exercises the actual Release session, exported file, Research session and both verified ledgers.
