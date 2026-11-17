# Shared foundation: proposed design v0.1

Status: specification, not running implementation. Decisions below are WraithKit's proposed contract, not guarantees inherited from an SDK.

## Small complete system
One Python package `wraithkit`, two separately runnable app folders, one SQLite database per owner/workspace. Reuse Pydantic AI for model requests and typed tool/output handling. Keep provider SDK objects inside one adapter; do not leak them into apps or history. No agent marketplace, distributed scheduler, vector database, arbitrary plugin loader, self-modifying policy or twenty simultaneous workers in v1.

Proposed repository shape:
```
packages/wraithkit/{model,tools,policy,store,runner,schemas}.py
apps/repo-wraith/       # independent entrypoint, config, README and fixtures
apps/research-wraith/   # independent entrypoint, config, README and fixtures
.kiro/{specs,steering,hooks}/  # actual future Kiro work, never fabricated usage
 tests/{contracts,policy,restart,apps}/
```
The current `demos/` folders contain design briefs only. No `.kiro` directory has been manufactured as proof of Kiro use. Kiro's documented spec structure separates requirements/design/tasks; migrate these drafts through an actual Kiro workflow and retain its genuine changes. [Kiro specs](https://kiro.dev/docs/specs/)

## Model interface
`ModelAdapter.generate(request, cancel) -> ModelTurn`
- Request: schema_version, run_id, model_ref, system_policy_version, bounded messages, registered tool schemas, requested output schema, token/time budget.
- Response discriminated union: `final(validated_output, usage)` OR `tool_calls(list[ToolCall], usage)` OR typed error. Reject an ambiguous final+tool response.
- Capabilities explicitly declare native structured outputs/tool calls/cancellation/context limit. Unsupported capability fails or follows an explicit, tested fallback, never silently drops schema/approval behavior.
- A provider-neutral tool call has a stable local ID, registered name/version and JSON arguments. Provider IDs are provenance, not authority.
- Validate input and output independently of model claims. Bound schema-repair attempts (maximum 2); exhausted validation ends the run as failed, not a successful free-text fallback.
- Proposed defaults: 8 model turns, 12 tool calls, 120 seconds/run, serial execution. All are configurable downward; usage/cost unknown is shown as unknown, never zero.
- Fixture adapter is visibly labeled synthetic/offline. Real-provider mode requires explicit configuration and data-egress permission. Never use production secrets in fixtures.

## Tool boundary and policy
Each registered tool declares input/output schema, version, effect (`read_local`, `read_public_web`, `write_workspace`, `external_mutation`), destinations, time/byte limits and idempotency behavior. The dispatcher, not the model, supplies owner/workspace IDs, credentials, policy and path roots.

Flow: validate call -> authorize scope -> compute canonical action digest -> persist proposal -> request approval when required -> revalidate latest policy/input snapshot -> claim action atomically -> execute bounded adapter -> persist receipt -> supply sanitized result to model.

Untrusted repository files, web pages, tool descriptions and remembered text never grant permissions. A tool result cannot register new tools or approve itself. MCP remains optional and must pass the same dispatcher; no automatic trust or credential passthrough.

Repository tools read a read-only snapshot/export. Deny traversal, absolute paths, Windows device/UNC paths, alternate streams and escaping symlinks/junctions. No Git hooks, submodules, package install or README commands execute during inspection. A later test runner needs an isolated filesystem, no secrets, network off and process limits; a Python path check is not a sandbox.

Research fetch uses allowlisted public HTTPS destinations, revalidates each redirect/DNS target and blocks loopback/private/link-local/metadata addresses and unexpected ports. Bound body/decompression bytes/time/MIME. Network failures are visible; search snippets alone cannot substantiate detailed claims.

## Durable state, separate from memory
SQLite tables proposed:
- runs(id,workspace_id,app,status,policy_version,created_at,revision)
- events(run_id,seq,type,public_payload,artifact_id,created_at), unique(run_id,seq)
- actions(id,run_id,tool_version,args_digest,input_snapshot,status,idempotency_key)
- approvals(id,action_id,owner_id,action_digest,policy_version,expires_at,status)
- memories(id,workspace_id,kind,content,source_refs,trust,revision,expires_at,revoked_at)
- artifacts(id,workspace_id,relative_path,sha256,mime,bytes,retention_until)

State transitions: created -> running -> waiting_approval -> running -> completed/failed/cancelled. An action interrupted after dispatch but before its receipt is **outcome_unknown**. On restart, never automatically repeat an irreversible action. Reconcile against a reliable idempotency receipt or require human recovery. There is no universal exactly-once guarantee across SQLite and an external service.

Approvals bind exact tool/version/arguments/destination/data classification and input snapshot; one-time CAS consumption prevents concurrent double-dispatch. Changed file hashes or policy invalidate old approval. Human identity comes from local trusted session, not model text. Denial/expiry cancels the action. First demos do not offer external mutations at all.

## Memory contract
Run context, durable event history and reusable memory are separate. Save reusable facts only through explicit owner-reviewed promotion. Store provenance, scope, trust and expiry; retrieval never crosses workspace. Source-derived facts remain untrusted, even after summarization. Corrections create revisions; revoked records are excluded from retrieval and pending actions revalidate referenced revisions. No automatic credential/health/identity-document memory.

First retrieval: SQLite text/FTS matching with bounded top-k and source links; no embedding service needed. Clearing memory differs from deleting run history/artifacts. Provide both controls and a truthful retention/export policy. Backups/WAL/OS caches complicate secure erasure; do not promise forensic deletion.

## Structured outputs and evidence
App outputs use versioned JSON Schema, with `status`, evidence references, unsupported/unknown findings and concrete artifacts. A source reference records URL or repository-relative path, exact snapshot/hash, retrieval time and locator. Validate every referenced ID exists in this run. Factual support still needs evaluation: schema validity is not truth.

Repo findings include path/range/snapshot and proposed changes, never a fabricated passed test. Research claims distinguish supported/inferred/unverified/conflicted; contradictory sources remain visible. Do not store hidden chain-of-thought. History stores actions, evidence, short user-facing reasons, approvals, timings, errors and measured usage.

## Interface
One shared execution timeline: requested -> evidence acquired -> proposed action -> approval/denial -> actual result. Status is text+icon, not color alone. Keyboard operation, visible focus, screen-reader labels and reduced-motion support are required. A restrained ghost visual identity can decorate the apps; no animations may obscure evidence/approval differences. Repo app centers a tree/diff; Research app centers a claim/source matrix. Both expose memory and history clearly.

## Security and operational limits
A model/provider SDK does not enforce workspace isolation. A local owner can edit its SQLite file; ordinary hashes are integrity receipts, not tamper-proof audit. Server deployment would require separate authentication, tenant isolation, encryption/key management, backups, rate limits and independent review. Do not market the prototype as a secure multi-tenant service.
