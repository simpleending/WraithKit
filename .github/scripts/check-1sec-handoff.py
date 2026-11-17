"""Synthetic export -> real import/check CLI. No engine, model, deployment or live data."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
SENTINEL = "SYNTHETIC_OPAQUE_FIELD_MUST_NOT_BE_COPIED"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=REPO / "target" / "debug" / ("security-wraith.exe" if os.name == "nt" else "security-wraith"))
    args = parser.parse_args()
    binary = args.bin.resolve(strict=True)
    fixtures = REPO / "apps/security-wraith/fixtures/1sec-export"
    inputs = {name: json.loads((fixtures / name).read_text(encoding="utf-8")) for name in ["events.json", "alerts.json", "mapping.json"]}
    env = {k: v for k, v in os.environ.items() if not k.startswith(("WRAITH_", "OPENAI_", "ANTHROPIC_"))}

    def run(*command, ok=True):
        result = subprocess.run([str(binary), *map(str, command)], capture_output=True, text=True, timeout=20, env=env)
        if (result.returncode == 0) != ok:
            # Do not print imported evidence or environment values on failure.
            raise AssertionError(f"{command[0]} returned unexpected exit code {result.returncode}")
        return result

    temp_parent = Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir())).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="wraith-1sec-ci-", dir=temp_parent) as name:
        root = Path(name).resolve()
        assert root.parent == temp_parent  # cleanup owns only this newly created temporary directory
        source = root / "export"
        source.mkdir()
        events = copy.deepcopy(inputs["events.json"])
        alerts = copy.deepcopy(inputs["alerts.json"]["alerts"])
        # Add explicit adversarial opaque fields to copies of the upstream-module synthetic fixture.
        for event in events:
            event.update(raw_data=SENTINEL, sensor={"credential": SENTINEL}, details={"token": SENTINEL})
        for alert in alerts:
            alert["metadata"] = {"token": SENTINEL}
        mapping = copy.deepcopy(inputs["mapping.json"])
        inputs = {"events.json": events, "alerts.json": {"alerts": alerts, "total": len(alerts)}, "mapping.json": mapping}
        for filename, value in inputs.items():
            (source / filename).write_text(json.dumps(value), encoding="utf-8")
        before = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in source.iterdir()}
        output = root / "new-incident"
        run("import-1sec", source, output)
        assert sorted(p.name for p in output.iterdir()) == ["incident.json"]
        body = (output / "incident.json").read_bytes()
        bundle = json.loads(body)
        assert bundle["provenance"] == "user_supplied_export" and bundle["sanitized"] is True
        assert bundle["ingestion"] == [] and SENTINEL not in body.decode("utf-8")
        assert bundle["asset"] == mapping["asset"]
        selected = next(a for a in bundle["alerts"] if a["id"] == mapping["selected_alert_id"])
        assert selected["metadata"]["asset_id"] == mapping["asset"]["id"]
        run("check", output)
        run("import-1sec", source, output, ok=False)
        assert (output / "incident.json").read_bytes() == body
        assert sorted(p.name for p in output.iterdir()) == ["incident.json"]
        assert before == {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in source.iterdir()}

        existing_file = root / "existing-file"
        existing_file.write_bytes(b"owned sentinel")
        run("import-1sec", source, existing_file, ok=False)
        assert existing_file.read_bytes() == b"owned sentinel"
        for case, filename, value in [
            ("unsanitized", "mapping.json", {**mapping, "sanitized": False}),
            ("missing-alert", "mapping.json", {**mapping, "selected_alert_id": "../../outside"}),
            ("provenance-override", "mapping.json", {**mapping, "provenance": "synthetic_fixture"}),
            ("no-alerts", "alerts.json", {"alerts": [], "total": 0}),
            ("oversized", "events.json", "x" * 65537),
        ]:
            path = source / filename
            original = path.read_bytes()
            path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
            rejected = root / case
            run("import-1sec", source, rejected, ok=False)
            assert not rejected.exists(), case
            path.write_bytes(original)

        if os.name != "nt":
            fifo = source / "mapping.json"
            original = fifo.read_bytes()
            fifo.unlink()
            os.mkfifo(fifo)
            run("import-1sec", source, root / "fifo-input", ok=False)
            assert not (root / "fifo-input").exists()
            fifo.unlink()
            fifo.write_bytes(original)

        # Unix CI must exercise the linked-input boundary; Windows may lack symlink privilege.
        path = source / "events.json"
        path.rename(source / "events-original.json")
        try:
            path.symlink_to(source / "events-original.json")
        except OSError:
            if os.name != "nt":
                raise
            print("Windows input-symlink case skipped: symlink privilege unavailable")
        else:
            run("import-1sec", source, root / "linked-input", ok=False)
            assert not (root / "linked-input").exists()
            path.unlink()  # the symlink this check created, not its target
            (source / "events-original.json").rename(path)
            other = root / "other-directory"
            other.mkdir()
            (other / "marker").write_bytes(b"unchanged")
            link = root / "linked-output"
            link.symlink_to(other, target_is_directory=True)
            run("import-1sec", source, link, ok=False)
            assert sorted(p.name for p in other.iterdir()) == ["marker"]
            assert (other / "marker").read_bytes() == b"unchanged"
        assert not list(root.rglob("response-plan.json"))
    print("PASS synthetic 1-SEC import/check, opaque-field filtering and no-overwrite/refusal boundaries; no model or enforcement")


if __name__ == "__main__":
    main()
