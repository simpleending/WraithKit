# Twenty Wraiths: staged product roadmap, not twenty implemented agents

Only 1 and 2 belong in the initial build. Every later Wraith must reuse shared policy/history/memory and earn a concrete task-specific evaluation. No autonomous worker swarm is implied.

| # | Wraith | Distinct useful output | Gate before implementation |
|---|---|---|---|
|1|Repo|Source-cited map and reviewed patch|Snapshot isolation, stale-input approval|
|2|Research|Claim/source matrix and concise brief|Evidence validity, SSRF, contradictions|
|3|Issue|Deduplicated issue triage with reproduction plan|No auto-label/close/post without approval|
|4|Test|Focused tests and actual execution receipts|Disposable runner, resource/secret isolation|
|5|Docs|Code-linked documentation drift report|Versioned source citations, no invented API|
|6|Release|Evidence-based changelog and release checklist|No tag/publish/deploy authority by default|
|7|Dependency|Upgrade impact and license/change evidence|Pinned advisories, no blind installation|
|8|Log|Bounded failure timeline from sanitized logs|Secret redaction, event ordering uncertainty|
|9|Incident|Human-reviewed incident brief/runbook|No automatic containment or blame attribution|
|10|Data|Profile/quality report for a selected dataset|PII minimization, sample-bias disclosure|
|11|SQL|Read-only query plan and schema explanation|Read credentials, cost/time limits, no writes|
|12|API|Contract-drift report and test proposals|Known endpoints only, no production mutation|
|13|Content|Source-backed draft in an approved voice|No fabricated endorsements or automatic posting|
|14|Meeting|Decisions/action draft from approved transcript|Participant consent, no invented commitments|
|15|Support|Grounded response draft from approved knowledge|No refunds, account access or sends by default|
|16|Policy|Control/evidence gap map|Not legal/compliance certification|
|17|Access|Accessibility findings and reproducible checks|Manual assistive-tech review, no score overclaim|
|18|Migration|Versioned migration plan and rollback rehearsal|Disposable copies, irreversible changes require approval|
|19|Cost|Usage/cost attribution and budget anomalies|Actual measured usage; unknown prices stay unknown|
|20|Sec|Authorized defensive evidence triage, optional 1-SEC adapter|License/auth/source gates; no exploitation or auto-remediation|

Wave A: 1-2. Wave B: 3-7 only after both apps are genuinely useful. Wave C: 8-12 when data/privacy/tool isolation is proven for those adapters. Wave D: 13-19 after audience validation. Wraith Sec is optional and may remain an independent application, not a security sticker for the core.

Each new Wraith needs one owner, one measurable user problem, one minimal tool allowlist, one output schema, benign/adversarial fixtures, an approval story and a retirement criterion. If it is only a different system prompt, ship it as a preset, not a new product claim.
