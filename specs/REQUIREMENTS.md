# Shared requirements and acceptance plan
All items are proposed and NOT YET IMPLEMENTED. Checkboxes remain unchecked until evidence is attached.

- [ ] R1: both independent apps use the same model/tool/policy/store package; replacing fixture provider changes no app logic.
- [ ] R2: malformed/extra-field tool arguments and final outputs are rejected before effects; capability mismatch produces a typed error.
- [ ] R3: unknown tool, traversal/junction escape, private-network fetch and prompt-injected authority requests are denied by dispatcher tests.
- [ ] R4: approval binds owner, workspace, tool version, arguments, destination and input hash; modified/replayed/expired/cross-workspace approval cannot execute.
- [ ] R5: two concurrent dispatchers cannot both claim one action; denied approval never runs the tool.
- [ ] R6: process restart after proposal, approval, dispatch and receipt is tested separately; uncertain side effect never silently retries.
- [ ] R7: memory retains provenance, revision, trust and expiry; poisoned source cannot become owner instruction; cross-workspace retrieval is refused.
- [ ] R8: memory revoke/delete and run-history deletion are distinct visible operations; stale memory revision invalidates pending use.
- [ ] R9: every material Repo finding names a real fixture snapshot/path/range; every Research claim links existing evidence or explicitly says unsupported.
- [ ] R10: budgets/cancellation stop new work; timed-out tools report failure and owned subprocesses are cleaned without touching unrelated processes.
- [ ] R11: no API keys/raw credentials in prompts, ordinary logs, artifacts or exports; provider egress is disclosed and opt-in.
- [ ] R12: both apps survive model refusal, missing network, invalid JSON, empty evidence, conflicting evidence, duplicate clicks and resumed approvals.
- [ ] R13: keyboard-only, readable contrast, screen-reader labels, narrow layout and reduced motion are manually tested. Automated checks alone are not certification.
- [ ] R14: live-provider results and deterministic fixture playback are labeled distinctly. Never use fixture success as a model-quality measurement.
- [ ] R15: actual Kiro work and external-code provenance are documented; no fake build history or fabricated usage.

Evaluation sets:10 benign and10 adversarial cases per demo initially; hold back at least4 cases per demo from prompt tuning. Record exact prompt/config/model/tool versions, dataset hash and all failures. Report evidence precision, unsupported-claim rate, tool-policy violations, approval correctness, restart outcomes and measured latency separately. Do not invent target pass rates as achieved metrics.

Hard gates: zero observed unauthorized dispatch in the bounded suite, no lost-denial/replayed approval, no claim of tests run without receipt, and correct refusal on unavailable evidence. Passing finite tests is not a universal safety guarantee.
