# Release Wraith

**A failed release, evidence you can inspect, and one exact fix you approve.**

Release Wraith uses WraithKit's existing evidence ledger, approval gate, model adapter and local UI. It does not copy those components. The bundled demo is a scripted model over synthetic CI records and a tiny synthetic ZIP. The local validation and approved file write really execute.

## Run the offline demo

From the repository root:

~~~sh
cargo run --locked --offline -p release-wraith -- demo
~~~

This requires cached dependencies and an installed Rust toolchain. Omit --offline only when you intend Cargo to download missing dependencies. Open the complete localhost URL printed by the process. The UI labels the model **FIXTURE: scripted, not a live model**.

1. Read the failed upload's CI record and log, the release plan and artifact inventory.
2. The initial local check reports FAIL: the plan names dist/spectral-cli.zip, but the verified local archive is artifacts/spectral-cli-1.4.2.zip.
3. Inspect the cited diagnosis and P1, the only proposal. It shows the entire exact release-plan.json before/after diff, including JSON formatting changes.
4. Approve P1 to change only that local plan. Deny P1 to leave all files untouched.
5. The script runs the local check again. Approval produces PASS; denial leaves FAIL.
6. Verify the ledger using the run folder printed by the app:

~~~sh
cargo run --locked --offline -p release-wraith -- verify .wraith/runs/<run-folder>
~~~

The CI log deliberately includes an untrusted instruction addressed to AI agents. It appears as flagged evidence, not authorization. This scripted scenario demonstrates the approval boundary; it is not a measured evaluation of a live model's resistance to prompt injection.

**PASS means only that the plan names an actual local file whose byte length and SHA-256 match the supplied inventory.** It does not mean that the archive is safe, contains a valid application, or that any build, test, hosted CI, upload or deployment ran. The historical CI record remains failure.

## Check your own local export without a model

~~~sh
cargo run --locked --offline -p release-wraith -- check /path/to/bundle
~~~

This command only reads files and prints JSON. It needs no API key and never starts a model or network request.

Exit codes:

| Code | JSON status | Meaning |
|---|---|---|
| 0 | PASS | The plan names a verified local artifact. |
| 1 | FAIL | The bundle is valid, but its plan names no artifact in the verified inventory. |
| 2 | INVALID | Invalid usage, malformed/missing input, disallowed path, unsupported schema, or changed/mismatched artifact bytes. |

Output always includes scope = LOCAL RELEASE CONTRACT ONLY, ci_rerun = false and published = false. A valid report also includes source, run_id, historical_ci_conclusion, planned_artifact, artifact_present_and_sha256_verified, verified_local_artifacts, reason and not_checked. Invalid input reports error instead.

### Bundle contract

Prepare a folder with four UTF-8 documents and the actual artifacts:

~~~text
bundle/
  ci-run.json
  ci.log
  release-plan.json
  artifact-index.json
  artifacts/
    your-package.zip
~~~

Copy the schemas from fixtures/bundle, replacing the synthetic metadata, hashes and artifacts with your own local exports. All three JSON documents reject unknown and duplicate fields. Use fixture = false consistently for your own export; the app labels it **USER-SUPPLIED LOCAL EXPORT; NOT INDEPENDENTLY FETCHED**. Records supplied by a user are not independently authenticated against a CI provider.

ci-run.json:

~~~json
{
  "schema": "wraith-ci-export-v1",
  "fixture": false,
  "run_id": "your-failed-run-id",
  "commit": "1111111111111111111111111111111111111111",
  "conclusion": "failure",
  "failed_step": "upload-artifact"
}
~~~

The commit must be 40 hexadecimal characters. This app currently investigates failed runs only.

release-plan.json:

~~~json
{
  "schema": "wraith-release-plan-v1",
  "fixture": false,
  "package": "your-package",
  "version": "1.0.0",
  "artifact": "dist/old-path.zip"
}
~~~

artifact-index.json:

~~~json
{
  "schema": "wraith-artifacts-v1",
  "fixture": false,
  "artifacts": [
    {
      "path": "artifacts/your-package.zip",
      "sha256": "<actual 64-character lowercase SHA-256>",
      "bytes": 123
    }
  ]
}
~~~

Replace both digest and byte length with the actual artifact values. The placeholder above is intentionally invalid. The app reads the actual artifact and checks both, rather than trusting the inventory alone.

Limits: four documents at most 64 KiB each; 1â€“16 artifacts, at most 8 MiB each and 32 MiB declared total. Artifact paths use portable alphanumeric, slash, dot, underscore and hyphen characters strictly under artifacts/. Traversal, absolute paths, alternate streams, reserved device names, symlinks and Windows reparse points in bundle-relative paths are refused.

## Optional live-model reasoning

The shared CLI also supports an explicitly selected live model:

~~~sh
cargo run --locked --offline -p release-wraith -- run /path/to/bundle "Investigate this release failure and propose the local artifact-path repair" --live
~~~

Use the kernel's documented model configuration. Read its data-sharing disclosure: the model receives the task and probed local document contents. Do not include secrets in your CI exports. This option has **not been exercised against a live provider for Release Wraith**. It changes only the reasoning provider; it does not connect to a CI service or grant workflow-execution capability.

## Tool boundary

| Tool | Kind | Scope |
|---|---|---|
| read_ci_failure | Probe | Reads ci-run.json and ci.log as separately cited evidence. |
| read_release_input | Probe | Reads only release-plan.json or artifact-index.json. |
| check_release_contract | Probe | Parses the bundle and hashes actual local artifact bytes. |
| fix_artifact_path | Effect | Changes only release-plan.json's artifact field to one verified local inventory path, after approval. |

The effect accepts only from and to. It cannot select another file or change another JSON field. Its exact proposed output is displayed. Preview inputs bind all four documents, every declared artifact's contents and each resolved destination. Any changed dependency invalidates approval, including a redirected target on resume. The write uses an exclusive temporary file and atomic replacement; it rechecks inputs before replacement.

This is an application-level boundary for a local single-user workflow, **not an OS sandbox, a filesystem transaction against malicious concurrent writers, a CI authorization service, or a guarantee of semantic correctness**. The final recheck reduces races but does not eliminate all filesystem race windows. No shell, process-execution, Git, workflow, upload or publication tool is registered.

## App checks

~~~sh
cargo test --locked --offline -p release-wraith
cargo clippy --locked --offline -p release-wraith --all-targets -- -D warnings
cargo fmt -p release-wraith -- --check
~~~

The tests exercise approval/denial, digest mismatch, repeated decisions, stale documents and artifacts, coherent artifact+inventory replacement, resume, target alias retargeting, link/junction refusal, external hardlink preservation, strict arguments, path/size/encoding guards, truthful local-export labels, the scripted injection annotation, ledger verification, CRLF preservation and actual CLI exit codes. Platform-specific filesystem checks run only where supported; a directory-link creation refusal prints SKIP rather than claiming coverage.


## Optional WraithBridge handoff

`release-wraith export-bridge <local-bundle> <new-handoff.json>` reruns the real local contract check and exports its bounded report plus SHA-256 source-content manifest. It creates a new file exclusively and refuses to overwrite anything. PASS and FAIL are both useful evidence; malformed bundles are refused before creating the output. No raw CI log, absolute filesystem root, credential, model task, tool call or approval is deliberately included. Selected report strings are still user-supplied: review them for private data before sharing.

The receiving app can run `research-wraith bridge <new-handoff.json>` to propose a quoted local release brief, requiring a **fresh Research approval**. The optional path does not alter ordinary Release runs. See [the Research bridge guide](../research-wraith/BRIDGE.md) for the full two-app flow, resume, limits and negative checks.
