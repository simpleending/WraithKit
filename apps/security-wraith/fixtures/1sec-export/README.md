# Captured upstream-module output, synthetic telemetry

`events.json` and `alerts.json` are unchanged JSON bytes emitted during the 2026-11-17 compatibility check against the user's clean local 1-SEC checkout at commit `2f6527d3f5e760b0e17955d75043dfb370f75d54`.

The independently authored [probe](../../tools/1sec-export-probe.go) called the real `auth.Fortress.HandleEvent` with a synthetic `auth_downgrade` observation (passkey to password, fake `synthetic-release-bot`, documentation-only IP `192.0.2.44`). The real module produced the HIGH `auth_method_downgrade` alert and its referenced generated event. An embedded loopback NATS bus captured the event; upstream Go JSON marshaling produced both files. The `{alerts,total}` wrapper matches `GET /api/v1/alerts`, but **no HTTP API request was made**.

The full engine, API server, cloud reporting, model adapters and enforcement/response handlers were never started. The existing Go 1.27.3 container ran with networking disabled, a read-only source snapshot, one CPU and 1 GiB RAM. Build and NATS scratch used separate disposable disk paths. No production identity or telemetry was used.

`mapping.json` is explicitly authored test context. The asset association is an operator assertion, not a field authenticated by upstream. The selected alert is a single detector alert, not a claim that the upstream cross-module correlator ran. The existing bundle field `correlated_alert_id` receives this selected investigation alert ID.

The empty upstream event `source` is intentional: this revision's module-created event leaves it empty. The importer adds the explicit `operator-supplied-1sec-export` transport label, not a guessed sensor identity. Opaque event details (including the original-event reference), raw binary/sensor fields and alert metadata are omitted. The original captured files remain here for comparison. The retained summaries, descriptions and other text are **not automatically redacted**.

CI replays these captured synthetic exports through the real Rust import/check CLI. It does not start 1-SEC on every CI run. To regenerate, place a copy of the probe at `cmd/wraith-export-probe/main.go` in a disposable checkout of that exact 1-SEC revision, use its declared Go dependencies, and run it inside an isolated network-disabled container with a fresh writable `/output` directory. It writes only synthetic export/receipt files and embedded-bus scratch there; do not attach production data or configuration. Upstream 1-SEC source/dependencies are not bundled in WraithKit and retain their own licenses.
