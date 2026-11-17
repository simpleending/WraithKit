---
inclusion: always
---
# WraithKit product

WraithKit is a small Rust kernel for AI agent apps ("Wraiths"). It is entered in Kiroween 2026, category **Skeleton Crew**: one lean template, proven by two distinct apps in separate folders (`apps/repo-wraith`, `apps/research-wraith`). Bonus targets: Social Blitz and Blog. The source of truth for scope is `.kiro/specs/wraithkit/`.

## What makes it different
Each idea below exists elsewhere. WraithKit's contribution is putting all of them into one template small enough to read, and having both apps reuse every one:
1. **Sealed effects.** Tools that change anything receive arguments only as `Clean<T>`, and only the kernel can create one, after a human approves. An app crate cannot bypass this; `compile_fail` tests prove it.
2. **Approvals cover data history.** An approval is bound to a digest of the tool, the exact arguments, the current hash of every input the effect touches, and the list of evidence that influenced the model. If any of these changes, the approval goes stale and the action does not run.
3. **One verifiable ledger for claims and actions.** Every quote, claim, proposal, decision and effect goes into one append-only, hash-chained JSONL file. `verify` rechecks it offline. Claims whose quote is not in the cited evidence are marked unsupported.

## Claims we never make
- "First", "unprecedented" or "provably immune to prompt injection". FIDES, CaMeL, lilith-zero, agentplane and the IETF agent-receipt drafts all exist.
- "Tamper-proof". The ledger is tamper-*evident* relative to a known head hash. It is not signed or anchored.
- "A verified quote means the claim is true". It only shows the quote is present in the stored snapshot.
- Fixture playback presented as live model behavior, or any metric we did not measure.

## Users
- A developer who wants an agent to diagnose and fix something in a repo without trusting it blindly.
- A builder who wants a cited answer that shows conflicts between sources.
- Judges: they must be able to run a fully offline demo in under a minute.

## Out of scope for the hackathon
Memory store, MCP, more than one model adapter, shell or test execution, a wasm/browser build, multi-user server deployment, and Wraiths 3-20. See `specs/ROADMAP_20.md` for later ideas.


## Current-scope addendum — 2026-11-17

This dated addendum supersedes the two-app scope and Wraiths 3–20 exclusion above for the current release. The earlier text remains as the original development context.

- Four runnable applications now reuse the same kernel. **Security Wraith** (`apps/security-wraith`) and **Release Wraith** (`apps/release-wraith`) are the main Skeleton Crew demonstrations. Repo and Research remain bonus examples, not claims of separate bonus-prize eligibility.
- Security reads bounded sanitized local incident exports and writes only a locally approved response plan. Its fixture pause and reversal are simulations; imported plans remain unexecuted. No live 1-SEC connector or enforcement is implemented.
- Release checks supplied local CI records and actual artifact bytes, then proposes a local release-plan path repair. Local PASS does not mean CI ran, the artifact is safe, or a release was published.
- Both primary apps have deterministic read-only check commands. Their optional live-model mode uses the existing adapter, not a security or CI connector. Fixture results are not model-performance evidence.
- The owner selected **MIT** for WraithKit on 2026-11-17. Dependencies retain their own licenses; 1-SEC is separately licensed and is not bundled.

Provenance: this scope maintenance and the new Security/Release extensions were ChatGPT/Cortana-assisted. This addendum is not evidence of a Kiro session. Preserve the original Kiro development record; record any future genuine Kiro review or implementation separately. Do not manufacture, backdate or infer Kiro usage.

## Handoff and public-site addendum - 2026-11-17

Security now has a tested `import-1sec` adapter for operator-redacted local event/alert exports plus explicit mapping. This is file-format compatibility, not a live connector or detector evaluation. WraithBridge now transfers a bounded, typed local Release report to Research as untrusted evidence; it never transfers approval. See `.kiro/README.md` and `BRIDGE.md` for the current implementation map and limits. The static landing is separate from the local Rust apps. These additions and this documentation are ChatGPT/Cortana-assisted, not a claim of new Kiro activity.
