# WraithBridge: a local handoff, with a new decision

WraithBridge is an optional part of the Rust library. One Wraith writes a typed report; another reads it as untrusted evidence. It does not carry an approval, run another app automatically, or grant tools to its recipient.

The first adapter connects **Release Wraith to Research Wraith**. Release checks actual local artifact bytes against a release plan. Research turns that report into a quoted brief and waits for a fresh approval before saving it. Both use the existing Session, evidence ledger and approval UI.

## Run the two-app walkthrough

Build the workspace once with Rust 1.96 or newer:

```sh
cargo build --workspace --locked
cargo run --locked --offline -p release-wraith -- demo
```

Open the printed local URL. Approve the proposed artifact-path repair. The local check now passes, although the historical CI run still failed and no CI was rerun. Copy the bundle directory shown as the run's target.

In another terminal, create a private local folder outside your checkout, then export to a **new** filename. Substitute the two local paths below:

```sh
release-wraith export-bridge <release-bundle-directory> <private-folder>/handoff.json
research-wraith bridge <private-folder>/handoff.json
```

Use binaries from `target/debug/` (add `.exe` on Windows), or prefix each command with `cargo run --locked --offline -p <app> --`. The output folder must already exist. Export refuses to overwrite a file and does not alter the Release bundle.

Research opens its own local UI. Read the imported evidence and the full brief preview. **Nothing is saved because you approved the Release repair.** Approve Research's separate proposal to write `briefs/release-handoff.md` beside the handoff file, or deny it to leave the output absent. The brief quotes the report and its input-hash manifest, identifies the evidence as untrusted, and retains the historical failure and unperformed checks.

```sh
research-wraith bridge-resume <research-run-directory>
research-wraith verify <research-run-directory>
```

The bridge's model sequence is deterministic and offline, even for a user-supplied Release export. It is not live-model reasoning or a model-quality evaluation. Research's ordinary research demo and optional live URL research remain separate commands. The bridge exposes no URL fetch, process execution, upload or publication tool.

## What the library checks

`wraithkit::bridge` works on bytes, not files or sockets:

- Envelope schema `wraithbridge`, version `1`, with an explicit producer, recipient and artifact type.
- A typed receiver-owned payload. Unknown and duplicate JSON fields, invalid UTF-8, excessive nesting, unsafe numbers, and envelopes over 7,500 bytes are rejected. Numbers are safe JSON integers; represent other numeric formats explicitly in your own schema.
- A source-kind assertion (`synthetic-fixture` or `user-supplied-export`) and 1–32 labeled SHA-256 input digests.
- A SHA-256 digest of the canonical packet, including its routing labels, provenance and payload.

The receiver chooses its expected producer, recipient and type in local code. It does not take these expectations from the incoming packet. The return type is `Received<T>`, **not `Clean<T>`**. `evidence_text()` retains the entire canonical envelope for the receiving app's own `EvidenceSink`.

The Release adapter uses type `release-contract-report` and exports only the local checker report and content hashes. It does not export raw CI logs, credentials, absolute source paths or an approval token. Retained report strings are not automatically scrubbed of secrets: review your data before handing it off.

## Trust and replay limits

A digest detects inconsistent bytes; it does not authenticate the sender. Anyone who can rewrite a packet can calculate another digest. Producer names, source markers and the source-hash manifest are assertions. Research does not possess or independently recheck the original Release artifacts. If your workflow requires identity or an independently pinned hash, establish that through a separate trusted channel.

Research pins the handoff bytes when opening a run. Its preview binds the handoff content, resolved input path, complete brief and output destination. Changed input is refused, and pending decisions are checked again on resume. Saving a brief goes through the normal exact-action approval gate. Replaying the same decision cannot execute it again.

You can intentionally open the same packet in another run. That run needs its own approval. This is not a global replay registry, a signed chain of custody, an automatic workflow scheduler or an operating-system sandbox. Use private, ordinary local directories that no other process is changing concurrently. Static symlinks, reparse points and special input files are refused; filesystem checks do not defend against a hostile concurrent process racing every operation.

## Add your own adapter

1. Define a small payload struct with Serde `deny_unknown_fields`. Give its contract a stable artifact type and validate its domain rules.
2. In the producer, gather the approved or read-only result and source content hashes. Call `bridge::encode`; write the result to a user-selected, new local file with bounded, no-overwrite I/O.
3. In the receiver, bounded-read the file and call `bridge::decode::<YourPayload>` with expectations fixed by the receiver. Validate the typed payload before offering it to a model.
4. Record the complete envelope through `EvidenceSink::add`. Treat embedded paths, URLs and instructions as inert data. Do not turn packet fields into tool registrations, network requests or permissions.
5. Declare any downstream write as an `Effect`. Bind the received bytes and actual destination in `Preview.inputs`, then require a new `Clean<Value>` through the normal Session decision.
6. Test malformed inputs, modified evidence, rejection, stale and repeated decisions, resume, and the real two-app path. Do not reuse a sender's ledger events as receiver decisions.

See `wraithkit/src/bridge.rs`, `apps/release-wraith/src/bridge.rs`, and `apps/research-wraith/src/bridge.rs`. No new dependency or daemon is required.

## Verification

`.github/scripts/check-wraithbridge.py <binary-directory>` runs two actual binaries: it approves a Release demo repair, exports the resulting report, and exercises Research approval, denial, stale input and resume over the real loopback UI API. It checks the saved file, refuses a repeated decision and tampered packet, and verifies each run's ledger. The main CI executes this on Windows, Linux and macOS alongside the four original app smoke flows. A workflow definition alone is not proof of a successful run; check Actions for the specific commit.

The bridge and its illustration were added with ChatGPT/Cortana on 2026-11-17. They are not attributed to the earlier Kiro sessions.
