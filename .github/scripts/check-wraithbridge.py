"""Real two-binary offline handoff; only temporary synthetic files and loopback HTTP."""
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request


def require(condition, message):
    if not condition:
        raise AssertionError(message)


class App:
    def __init__(self, exe, args, cwd):
        env = os.environ.copy()
        env.pop("WRAITH_API_KEY", None)
        for key in ("TEMP", "TMP", "TMPDIR"):
            env[key] = str(cwd / "tmp")
        self.process = subprocess.Popen([str(exe), *map(str, args)], cwd=cwd, env=env,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        text=True, encoding="utf-8")
        self.base = None
        self.run = None
        lines = queue.Queue()
        def reader():
            for line in self.process.stdout:
                lines.put(line.rstrip())
            lines.put(None)
        threading.Thread(target=reader, daemon=True).start()
        try:
            for _ in range(20):
                line = lines.get(timeout=20)
                require(line is not None, "app exited before its UI was ready")
                if line.startswith("run folder: "):
                    self.run = cwd / line.removeprefix("run folder: ")
                if re.fullmatch(r"open http://127\.0\.0\.1:\d+/[a-f0-9]+/", line):
                    self.base = line.removeprefix("open ").rstrip("/")
                    break
            require(self.base and self.run, "missing loopback UI or run directory")
        except BaseException:
            self.close()
            raise

    def request(self, path, body=None):
        req = urllib.request.Request(self.base + path,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=10) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            return {"http_error": error.code}

    def state(self, expected):
        for _ in range(150):
            state = self.request("/api/state")
            if state["status"] == expected:
                return state
            require(state["status"] not in ("failed", "needs_review"), str(state))
            time.sleep(.05)
        raise AssertionError("timed out waiting for " + expected)

    def decide(self, proposal, approve=True):
        return self.request("/api/decide", {"proposal": proposal["id"],
            "digest": proposal["digest"], "approve": approve})

    def verify(self):
        result = self.request("/api/verify", {})
        require(result["all_ok"] and len(result["checks"]) == 7, "ledger verification failed")

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.process.stdout.close()
        self.process.stderr.close()


def run(bin_dir):
    suffix = ".exe" if os.name == "nt" else ""
    release = bin_dir / ("release-wraith" + suffix)
    research = bin_dir / ("research-wraith" + suffix)
    with tempfile.TemporaryDirectory(prefix="wraithbridge-smoke-") as folder:
        root = Path(folder).resolve()
        (root / "tmp").mkdir()
        producer = App(release, ["demo"], root)
        try:
            state = producer.state("waiting")
            target = Path(state["target"])
            proposal = next(p for p in state["proposals"] if p["id"] == "P1")
            require(producer.decide(proposal)["outcome"] == "approved", "Release approval failed")
            producer.state("finished")
            producer.verify()
            plan = json.loads((target / "release-plan.json").read_text())
            require(plan["artifact"] == "artifacts/spectral-cli-1.4.2.zip", "Release did not repair the actual plan")
        finally:
            producer.close()
        for scenario in ("approve", "deny", "stale", "resume"):
            directory = root / scenario
            directory.mkdir()
            handoff = directory / "handoff.json"
            exported = subprocess.run([str(release), "export-bridge", str(target), str(handoff)],
                                      capture_output=True, text=True, timeout=20)
            require(exported.returncode == 0, exported.stderr)
            original = handoff.read_bytes()
            require(json.loads(original)["packet"]["payload"]["status"] == "PASS", "handoff was not from repaired Release bundle")
            duplicate = subprocess.run([str(release), "export-bridge", str(target), str(handoff)],
                                       capture_output=True, text=True, timeout=20)
            require(duplicate.returncode != 0 and handoff.read_bytes() == original, "export overwrote an existing handoff")
            consumer = App(research, ["bridge", handoff], root)
            try:
                state = consumer.state("waiting")
                output = directory / "briefs" / "release-handoff.md"
                require(not output.exists(), "producer approval leaked into receiver write")
                require(state["app"] == "research-wraith-bridge", "wrong consumer app")
                proposal = next(p for p in state["proposals"] if p["status"] == "pending")
                if scenario == "resume":
                    run_dir = consumer.run
                    consumer.close()
                    consumer = App(research, ["bridge-resume", run_dir], root)
                    state = consumer.state("waiting")
                    require(not output.exists(), "resume auto-approved the brief")
                    proposal = next(p for p in state["proposals"] if p["status"] == "pending")
                if scenario == "stale":
                    handoff.write_bytes(original + b"\n")
                answer = consumer.decide(proposal, scenario != "deny")
                if scenario in ("approve", "resume"):
                    require(answer.get("outcome") == "approved", str(answer))
                    consumer.state("finished")
                    brief = output.read_text(encoding="utf-8")
                    require("**PASS**" in brief and "Historical CI remains **failure**" in brief, "brief lost result or historical context")
                    require("untrusted" in brief and "No approval is transferred" in brief, "brief lost trust disclosure")
                    digest = hashlib.sha256(output.read_bytes()).hexdigest()
                    replay = consumer.decide(proposal)
                    require(replay.get("outcome") != "approved", "decision replay approved twice")
                    require(hashlib.sha256(output.read_bytes()).hexdigest() == digest, "decision replay changed brief")
                else:
                    expected = "denied" if scenario == "deny" else "stale:"
                    require(answer.get("outcome", "").startswith(expected), str(answer))
                    consumer.state("finished")
                    require(not output.exists(), scenario + " wrote a brief")
                consumer.verify()
                print("Release -> Research:", scenario, "fresh approval + exact artifact + ledger PASS", flush=True)
            finally:
                consumer.close()
        # A modified report cannot even start a receiver session.
        bad = root / "tampered.json"
        body = json.loads(original)
        body["packet"]["payload"]["status"] = "FAIL"
        bad.write_text(json.dumps(body), encoding="utf-8")
        refused = subprocess.run([str(research), "bridge", str(bad)], cwd=root,
                                 capture_output=True, text=True, timeout=20)
        require(refused.returncode != 0, "tampered packet accepted")
        print("Tamper and overwrite refusal PASS; no model, external service or CI run used.")


if __name__ == "__main__":
    require(len(sys.argv) == 2, "usage: check-wraithbridge.py <binary-directory>")
    run(Path(sys.argv[1]).resolve())
