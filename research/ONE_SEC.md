# 1-SEC: current source and Wraith Sec boundary

## Identity verification
Current public repository: [simpleending/1-sec](https://github.com/simpleending/1-sec), main commit `bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef`, committed2026-11-12T22:08:23Z, queried2026-11-15 through GitHub API. API tree is untruncated. Historical cutmob reference supplied by parent is not treated as current. `1sec-security/1sec` returned404 in this research. No separate current private1-SEC thread was recovered; this report is a pinned public-source review, not the team's full historical record.

Repository metadata and five pinned upstream snapshots/hashes are under `evidence/1sec-*`. They are research copies only; do not include them in a public WraithKit release automatically.

## What source actually supports
The README describes a security telemetry/response engine with16modules and NATS JetStream, optional Rust components, and prevention dependent on trusted observations, configuration, enforcing integration and host privileges. It is not a general-purpose agent foundation or proof of automatic protection. [Pinned README](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/README.md)

`internal/api/server.go` registers status/module/alert/history endpoints, event ingestion and separate enforcement controls. Its middleware distinguishes read/write credentials. `internal/core/response.go` includes approval handling that rechecks current policy and dry-run state. Those are code observations, not executed endpoint or penetration-test results. [API source](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/internal/api/server.go), [response source](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/internal/core/response.go)

## Licensing gate, not a legal conclusion
The LICENSE header offers AGPLv3 and commercial terms, then adds a no-rebranding/resale/competing-service restriction. GitHub metadata reports NOASSERTION rather than a recognized license identifier. Do not describe this as ordinary permissive code or assume copying/rebranding is cleared. The interaction between that extra clause, AGPL terms, contributor rights and Noble's actual ownership needs clarification before bundling or public integration. [Pinned LICENSE](https://github.com/simpleending/1-sec/blob/bf47bc6c7fe3e1c8b1fca8ad3be8e8982c5f83ef/LICENSE)

## Proposed optional integration, no implementation yet
Keep WraithKit independent. Wraith Sec starts as an **evidence explainer**, consuming owner-supplied sanitized1-SEC report fixtures, not as a scanner or enforcement engine. A later approved adapter may read a specifically configured1-SEC instance using read-scoped credentials and endpoint allowlists. Do not expose arbitrary API paths, write keys, token-vault exchange, approval/rollback/config/shutdown endpoints, shell, process killing or firewall changes as model tools.

Map upstream event IDs/source timestamps/version into WraithKit evidence records. Treat alert text as untrusted data. Distinguish observation, inference and recommended human action. A WraithKit approval is not an upstream1-SEC approval, nor vice versa. Missing/stale/truncated telemetry must be visible. Credential setup, data sharing, live network access and license review are separate gates.

Required future tests: unknown endpoint refused; no write method dispatched; poisoned alert cannot alter policy; stale event cannot be called current; missing sensor evidence cannot be upgraded to containment authority; keys redacted from output/history. No live1-SEC service was called or installed; no security scan or exploit run was performed.
