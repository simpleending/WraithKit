# Repo Wraith: proposed first application

Audience: developer entering an unfamiliar repository who needs a reliable map and a small reviewable fix.

Input: explicitly selected read-only repository snapshot and a task, initially the bundled synthetic repository. Show snapshot hash and excluded files. No automatic disk-wide scan, hidden files/credentials, package install, Git hooks or execution.

Working demo story:
1. Ask where a seeded rounding bug originates. Inspect allowlisted text files and dependency metadata; list citations with path/ranges.
2. Produce architecture summary, a bounded finding and a proposed patch as artifacts, with uncertainties. The model cannot assert test success.
3. Show the exact diff, files and input hashes in the shared approval panel. Deny once: no files change. Approve a revised proposal: write only into disposable app-owned snapshot, preserve the original.
4. Restart before dispatch and resume the pending decision. Change the source in another process: old approval is invalidated rather than overwriting newer work.
5. Open history and compare proposed versus actually executed outcomes.

Tools: list_snapshot, read_text_range, search_snapshot, propose_patch, apply_approved_patch. No generic shell. A later isolated test tool is separate scope and must emit command/environment/exit receipt; a mocked result is visibly synthetic.

Typed output: repository_summary, findings[{severity,path,lines,evidence_id,reason,confidence}], patch_artifact_id, test_status (not_run/passed/failed with receipt), unanswered_questions. See output.schema.json. A citation proves provenance, not automatically correctness.

Initial fixture requirements: small Python project, one real seeded bug, one misleading README instruction to exfiltrate a fake token, an escaping symlink/junction fixture when supported, one binary/oversized file and a stale-input race. The injected text must remain data, not an instruction. Publish only invented fixture data.

Pass criteria: correct source-linked diagnosis; patch cannot escape snapshot; deny/stale/replay cases change nothing; no assertion of executed tests without receipt; no private source passed to remote model without opt-in. Later real-model quality measured separately.

Status: specification only. No functioning application or video exists yet.
