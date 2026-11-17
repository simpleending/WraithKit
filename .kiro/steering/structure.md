---
inclusion: always
---
# WraithKit repository structure

```
Cargo.toml                    workspace: wraithkit + the two apps
Cargo.lock                    committed
LICENSE                       owner picks (MIT recommended); must be detected by GitHub
README.md                     product README (rewritten in the packaging task)
BUILD_LOG.md                  honest dated log of Kiro work; disclose pre-existing material
.kiro/steering/               product, tech, structure (this file)
.kiro/specs/wraithkit/        requirements.md, design.md, tasks.md  <- source of truth
.kiro/hooks/                  Kiro hooks
.github/workflows/            ci.yml, release.yml
wraithkit/                    the kernel library (the "skeleton")
  src/lib.rs                  re-exports, sha256_hex, canonical, now_ms, args, limits
  src/taint.rs                Tainted, Clean, release (pub(crate))
  src/ledger.rs               Event, Line, Ledger, blobs
  src/app.rs                  App trait, ToolSpec, Kind, Preview, Demo, EvidenceSink
  src/model.rs                Model: Fixture | Live (OpenAI-compatible)
  src/session.rs              State fold, run loop, proposals, decide, resume
  src/verify.rs               offline ledger verifier
  src/server.rs               tiny_http UI + JSON API
  src/cli.rs                  demo | run | resume | verify
  ui/index.html               the one shared UI (vanilla HTML/CSS/JS)
apps/repo-wraith/             app 1: only App impl, prompt, fixtures
apps/research-wraith/         app 2: only App impl, prompt, fixtures
```

Placement rules:
- Code that both apps need goes in `wraithkit`. Code that one app needs stays in that app, including the URL/SSRF guard (Research) and path confinement (Repo).
- An app folder holds only `src/main.rs` (plus small modules if it grows past ~400 lines), `src/prompt.md`, `fixtures/`, `README.md` and `Cargo.toml`. If an app needs ledger, approval, model or UI code, that code belongs in the kernel.
- Runs are written to `.wraith/runs/<id>/` (ledger.jsonl + blobs/). `.wraith/` is gitignored.

Not part of the product:
- `research/`, `specs/`, `demos/`, `MANIFEST.json`, `AUDIT.json`, `HANDOFF.md` and `DIRECTION_UPDATE_RUST.md` are the dated pre-build research snapshot (Python v0 design, superseded). Keep them unchanged.
- `evidence/` holds third-party 1-SEC source snapshots. It is gitignored and must never be published.
- **Never add `.kiro` or any subfolder of it to `.gitignore`.** That can disqualify the entry, and CI checks for it.


## Current-layout addendum — 2026-11-17

The current workspace has **five crates**: `wraithkit` plus `apps/security-wraith`, `apps/release-wraith`, `apps/repo-wraith` and `apps/research-wraith`. Security and Release are the main demonstrations; Repo and Research remain bonus examples. The owner-selected MIT `LICENSE` and Cargo metadata are in place. See the current-scope addendum in `product.md` for feature boundaries and provenance.

The original layout above is retained as development history. All placement, security and historical-artifact preservation rules still apply. This update is ChatGPT/Cortana-assisted documentation maintenance, not a claim of Kiro activity.
