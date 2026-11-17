"""Check publication exclusions without reading credentials or changing Git state.

This is an ignore/packaging contract, not a secret scanner or publication approval.
Only a separately reviewed tracked snapshot should be published.
"""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def ignored(path):
    result = subprocess.run(
        ["git", "check-ignore", "--no-index", "-q", "--", path], cwd=ROOT
    )
    if result.returncode not in (0, 1):
        raise RuntimeError("git check-ignore failed")
    return result.returncode == 0


def main():
    excluded = [
        ".wraith/runs/private/ledger.jsonl", "apps/research-wraith/.wraith/run.json",
        "evidence/upstream-private.txt", "local/incident.json", "exports/events.json",
        "briefs/release-handoff.md", "dist/index.html", "public-build/index.html",
        "handoff.json", "response-plan.json", ".env", ".env.local",
        "apps/security-wraith/.env", ".cargo/credentials.toml", ".npmrc", ".netrc",
        "private.pem", "private.key", "id_ed25519", "target/debug/security-wraith",
        "node_modules/package/index.js", "playwright-report/index.html",
        "test-results/private.json", "security-wraith.exe", "release-wraith-linux.zip",
    ]
    required = [
        "Cargo.lock", "LICENSE", "THIRD_PARTY_NOTICES.md", "BRIDGE.md",
        ".github/workflows/ci.yml", ".kiro/steering/product.md",
        ".kiro/specs/wraithkit/requirements.md", ".kiro/hooks/rust-fmt-on-save.json",
        "apps/repo-wraith/fixtures/demo-repo/.env",
        "apps/release-wraith/fixtures/bundle/ci.log",
        "apps/release-wraith/fixtures/bundle/artifacts/spectral-cli-1.4.2.zip",
        "apps/security-wraith/fixtures/1sec-export/events.json",
        "site/assets/ledger.jsonl", "site/assets/wraith-bridge.webp",
        "site/fonts/fragmentmono-OFL.txt", "site/fonts/gloock-OFL.txt",
        "site/fonts/hostgrotesk-OFL.txt",
    ]
    for path in excluded:
        if not ignored(path):
            raise AssertionError("local/private path is not ignored: " + path)
    for path in required:
        if ignored(path) or not (ROOT / path).is_file():
            raise AssertionError("required source/evidence missing or ignored: " + path)
    tracked = git("ls-files", "-z").decode("utf-8").split("\0")
    for path in filter(None, tracked):
        if ignored(path):
            raise AssertionError("ignored local/private file is already tracked: " + path)
    print("Publication ignore contract passed; tracked source, fixtures, notices and .kiro retained.")
    print("Review the selected snapshot and history separately; .gitignore cannot redact secrets.")


if __name__ == "__main__":
    main()
