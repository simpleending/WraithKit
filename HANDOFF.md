# Current WraithKit handoff

Updated 2026-11-17. WraithKit is now an implemented Rust workspace, not the initial Python research proposal. Start with [README.md](README.md), then [SUBMISSION.md](SUBMISSION.md) for judge instructions, the Kiro-use draft and the under-three-minute demo plan.

## Implemented scope

- `wraithkit/`: shared approval gate, evidence and hash-chained ledger, model adapters, CLI, local UI and verifier.
- `apps/repo-wraith/`: repository probes and an approval-gated exact file edit.
- `apps/research-wraith/`: allowlisted source reads and an approval-gated saved brief.
- `.kiro/`: requirements, design, tasks, steering and hook configurations. [BUILD_LOG.md](BUILD_LOG.md) attributes historical implementation and validation to dated sessions and records later non-Kiro work separately.
- `site/`: a separate landing page, not a hosted version of the two apps.

The fixture demos use scripted responses and synthetic inputs. They are not live model evaluations. The build log records one owner-run live smoke test per app, not a reliability benchmark. Run histories and demo outputs are local; no public release or video is established by this handoff.

## Validation and publication boundary

The 2026-11-17 review completed 119 passing Windows tests, formatting and clippy checks. The three network tests ignored by the default suite were separately run and passed. Both Windows debug and release binaries also passed the automated CLI/HTTP fixture smoke check. One existing Repo file-symlink test returned early for missing Windows symlink privileges; the Research junction regressions did run. See README for the breakdown. The earlier hosted CI and release rehearsal used a private test repository, and this local result is not proof that the updated source passed hosted CI. Run and record final validation against the exact release commit before publishing.

MIT was selected by the owner for the hackathon on 2026-11-17. The existing `LICENSE` and Cargo metadata already match; no license text change was needed.

Remaining gates: owner eligibility, final public repository/test-build access, matching hosted CI and release checks, a genuine recorded video, owner-approved Kiro narrative, and any selected bonus material. The broader live-model evaluation, remaining manual accessibility checks and manual Linux/macOS demo checks are not claimed complete. See README for security limits, including trusted app code and the remaining filesystem/DNS race windows.

Keep `evidence/` private. It contains third-party research snapshots, not product implementation. Do not publish `.wraith/`, private session transcripts, credentials or raw live-run data. Review tracked files and staged changes before a public push. No registration, submission, public repository change or deployment is authorized by this document.

## Historical records

The original November 15 handoff below describes the research package **at that date only**. Its recommendation, implementation status and negative statements about actions are superseded by the current handoff and dated build log. `MANIFEST.json` and `AUDIT.json` describe that initial package, not the current Rust source or a security certification. The original handoff text is preserved below; its old hashes do not apply to this expanded file.

---

## Archived original handoff (2026-11-15)

# Research/specification handoff

Created 2026-11-15 in a new project folder. Read README.md first. This public copy omits the original local path and unrelated private-project references.

## Deliverables
- research/: primary-source foundation comparison, official Kiroween entry gates, current 1-SEC source/license analysis and URL index.
- specs/: shared architecture, acceptance requirements, ordered implementation tasks, schema caveats and 20-Wraith roadmap.
- demos/: two distinct application specifications with proposed output schemas and explicitly synthetic examples.
- evidence/: pinned current public 1-SEC metadata and source snapshots with original byte hashes. Private research only; not a proposed public redistribution bundle.

## Recommended next action
Build two small vertical slices on one Pydantic AI/SQLite foundation using authentic Kiro work. Start fixture-only and prove approval/restart/evidence boundaries, then connect one real model with explicit data-egress permission. Do not implement twenty apps or import 1-SEC as the core.

Owner eligibility, license selection, actual Kiro use and public access remain gates. No registration, submission, deployment, installs, credentials, model calls or desktop UI occurred. No hidden private conversation was recovered for 1-SEC; the report distinguishes current public source from the historical account lead.

## Verification scope
AUDIT.json records JSON syntax, local links, five pinned upstream hashes, twenty roadmap rows and absence of fabricated Kiro evidence. These are document/package integrity checks, not schema/runtime/security tests. No dependency versions were invented or installed. All output examples are synthetic, not demo executions.

MANIFEST.json hashes all payload files except itself. It is a local integrity index, not a signature or immutable external backup. No git commit/push was made. No owned process or resource reservation remains.
