# Kiro development records and current scope

Current-scope index, 2026-11-17. This index is ChatGPT/Cortana-assisted documentation maintenance, not evidence of a new Kiro session.

The original [requirements](specs/wraithkit/requirements.md), [design](specs/wraithkit/design.md), [tasks](specs/wraithkit/tasks.md) and hook configurations document the original Repo/Research development. Preserve them as history. The dated addenda in [product steering](steering/product.md) and [structure steering](steering/structure.md) supersede their original two-app scope. [BUILD_LOG.md](../BUILD_LOG.md) and [SUBMISSION.md](../SUBMISSION.md) distinguish genuine Kiro work from later assistance.

## Current implementation

- `wraithkit/`: shared Rust approval boundary, evidence ledger, model adapter, local UI and offline verifier. This is an application-level boundary, not an OS sandbox.
- `apps/security-wraith/`: primary example. Imports operator-redacted 1-SEC exports with explicit mapping and validates local bundles. Approved effects write a local response plan; imported plans remain `not_executed`. No live 1-SEC connector or containment.
- `apps/release-wraith/`: primary example. Checks local release records and artifact bytes; approval repairs a local release-plan path. It does not run CI, build or publish a release.
- `apps/repo-wraith/`: bonus example. Proposes a confined local file edit with fresh input checks and human approval.
- `apps/research-wraith/`: bonus example. Collects allowed-source evidence and proposes a cited local brief.
- [WraithBridge](../BRIDGE.md): optional typed, versioned Release-to-Research file handoff. The receiver imports untrusted evidence and requires its own approval. Hashes provide integrity, not sender authentication; there is no global replay registry.
- `site/`: static public landing and browser-only ledger demonstration. It does not execute or host the Rust apps.

The owner selected MIT. Third-party licenses/notices remain separate. Do not infer Kiro usage from the presence of these files or describe later Security/Release, importer, bridge or landing work as historical Kiro output. For current test results and platform limitations, use the linked CI runs in the root README, not historical test counts.
