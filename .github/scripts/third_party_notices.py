#!/usr/bin/env python3
"""Build release notices from the locked, locally cached Cargo source packages.

Uses only the Python standard library; never downloads or installs anything.
Run after dependencies and the Rust documentation component are available:
  python .github/scripts/third_party_notices.py
Validate checked-in notice coverage without Cargo/cache/toolchain access:
  python .github/scripts/third_party_notices.py --verify
Regenerate in memory and compare to the checked-in files:
  python .github/scripts/third_party_notices.py --check
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
NOTICE = ROOT / "THIRD_PARTY_NOTICES.md"
RUST_NOTICE = ROOT / "RUST_STANDARD_LIBRARY_NOTICES.html"
LICENSE_NAME = re.compile(r"^(?:LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)(?:[._-]|$)", re.I)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def verify() -> None:
    text = NOTICE.read_text(encoding="utf-8")
    lock_match = re.search(r"^Cargo.lock SHA-256: `([a-f0-9]{64})`$", text, re.M)
    rust_match = re.search(r"^Rust standard-library notice SHA-256: `([a-f0-9]{64})`$", text, re.M)
    if not lock_match or lock_match[1] != sha256((ROOT / "Cargo.lock").read_bytes()):
        raise RuntimeError("Cargo.lock changed or notices are missing: regenerate third-party notices")
    if not rust_match or rust_match[1] != sha256(RUST_NOTICE.read_bytes()):
        raise RuntimeError("Rust standard-library notices are missing or changed: regenerate notices")
    print("Release notice fingerprints match Cargo.lock and the bundled Rust notice.")


def generate() -> tuple[bytes, bytes]:
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1"))
    packages = sorted((p for p in metadata["packages"] if p.get("source")), key=lambda p: (p["name"], p["version"]))
    rust_version = run("rustc", "--version").decode().strip()
    sysroot = Path(run("rustc", "--print", "sysroot").decode().strip())
    rust_docs = sysroot / "share" / "doc" / "rust"
    rust_bytes = (rust_docs / "COPYRIGHT-library.html").read_bytes()
    if b"Copyright notices for The Rust Standard Library" not in rust_bytes:
        raise RuntimeError("Unexpected Rust standard-library notice format")

    texts: dict[str, str] = {}
    sections: list[str] = []

    def register(path: Path) -> str:
        raw = path.read_bytes()
        digest = sha256(raw)
        # Keep all words/copyright notices. Normalize only text newlines in the Markdown copy.
        text = raw.decode("utf-8-sig").replace("\r\n", "\n").replace("\r", "\n")
        texts.setdefault(digest, text)
        return digest

    sections.extend([
        "## Rust runtime base licenses\n",
        f"Toolchain used to collect these notices: `{rust_version}`.\n",
        "The exact standard-library copyright document is bundled as "
        "[RUST_STANDARD_LIBRARY_NOTICES.html](RUST_STANDARD_LIBRARY_NOTICES.html). "
        "It includes the standard library's own in-tree and third-party notices.\n",
    ])
    for name in ("MIT.txt", "Apache-2.0.txt"):
        digest = register(rust_docs / "licenses" / name)
        sections.append(f"- `{name}`: [license text](#notice-{digest})\n")
    sections.append("\n## Cargo packages\n")

    notice_files = 0
    for package in packages:
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise RuntimeError(f"Review non-crates.io dependency source before releasing {package['name']}")
        directory = Path(package["manifest_path"]).parent
        paths = {p for p in directory.rglob("*") if p.is_file() and LICENSE_NAME.match(p.name)}
        if package.get("license_file"):
            license_file = directory / package["license_file"]
            if not license_file.is_file():
                raise RuntimeError(f"Missing declared license_file for {package['name']}")
            paths.add(license_file)
        if not paths:
            raise RuntimeError(f"No license/notice files found for {package['name']} {package['version']}")
        name, version = package["name"], package["version"]
        sections.extend([
            f"\n### {name} {version}\n\n",
            f"Declared license expression: `{package.get('license') or 'See license files'}`.\n\n",
            f"Package source: https://crates.io/crates/{name}/{version}\n\n",
        ])
        if package.get("repository"):
            sections.append(f"Upstream repository: {package['repository']}\n\n")
        for path in sorted(paths, key=lambda p: p.relative_to(directory).as_posix()):
            digest = register(path)
            relative = path.relative_to(directory).as_posix()
            sections.append(f"- `{relative}`: [license/notice text](#notice-{digest})\n")
            notice_files += 1

    header = [
        "# Third-party notices\n\n",
        "WraithKit's own source is licensed under MIT; see LICENSE. Dependencies and "
        "bundled data retain their respective copyrights and licenses below.\n\n",
        f"Cargo.lock SHA-256: `{sha256((ROOT / 'Cargo.lock').read_bytes())}`\n\n",
        f"Rust standard-library notice SHA-256: `{sha256(rust_bytes)}`\n\n",
        f"Coverage: {len(packages)} external Cargo packages; {notice_files} upstream "
        "license/copyright/notice files, including nested notices.\n\n",
        "This intentionally covers the complete locked Cargo metadata graph, including "
        "build-time and target-specific packages. Not every listed package is present in "
        "every binary. All discovered license alternatives are preserved; this listing "
        "does not relicense dependencies. Identical upstream texts are deduplicated by "
        "SHA-256 of their original bytes, with each package/file mapped to its text.\n\n",
        "Collected offline from the cached published crate sources and the installed "
        "Rust documentation. Source license texts are copied, not paraphrased. The "
        "regenerator uses only Python's standard library and is at "
        "`.github/scripts/third_party_notices.py`. Run it again after changing Cargo.lock "
        "or the release toolchain. `--verify` checks the recorded file fingerprints; "
        "`--check` recollects the notices and compares their contents.\n\n",
        "The separately served website fonts have their own OFL license files and "
        "provenance notes in `site/fonts/`; they are not embedded in the app binaries.\n\n",
    ]
    output = header + sections + ["\n## License and notice texts\n"]
    for digest, content in sorted(texts.items()):
        fence = "`" * max(4, 1 + max((len(x) for x in re.findall(r"`+", content)), default=0))
        output.extend([
            f'\n<a id="notice-{digest}"></a>\n\n',
            f"### Notice `{digest}`\n\n",
            fence + "text\n", content, "" if content.endswith("\n") else "\n", fence + "\n",
        ])
    return "".join(output).encode("utf-8"), rust_bytes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--verify", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.verify:
        verify()
        return
    notices, rust_notices = generate()
    if args.check:
        if NOTICE.read_bytes() != notices or RUST_NOTICE.read_bytes() != rust_notices:
            raise RuntimeError("Generated notices differ from the checked-in files")
        print("Third-party notices exactly match the cached upstream license texts.")
        return
    NOTICE.write_bytes(notices)
    RUST_NOTICE.write_bytes(rust_notices)
    verify()
    print(f"Wrote {NOTICE.name} ({len(notices)} bytes) and {RUST_NOTICE.name} ({len(rust_notices)} bytes).")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"notice generation failed: {error}", file=sys.stderr)
        sys.exit(1)
