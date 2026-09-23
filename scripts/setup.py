#!/usr/bin/env python3
"""One-time setup: check tools, install frontend packages, create backend/.env
and import the static datasets.

    python scripts/setup.py              # everything
    python scripts/setup.py --skip-data  # no downloads
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys

from common import ROOT, log


def version(command: list[str]) -> tuple[int, ...] | None:
    try:
        out = subprocess.run(command, capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    match = re.search(r"(\d+)\.(\d+)\.(\d+)", out)
    return tuple(int(v) for v in match.groups()) if match else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--skip-data", action="store_true", help="do not download the static datasets")
    args = parser.parse_args()

    problems = []
    if sys.version_info < (3, 10):
        problems.append("Python 3.10 or newer")
    cargo = shutil.which("cargo") or shutil.which(os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo"))
    if not cargo:
        problems.append("Rust (https://rustup.rs)")
    node = version(["node", "--version"]) if shutil.which("node") else None
    if not node or node < (20, 19, 0) or (22, 0, 0) <= node < (22, 12, 0):
        problems.append("Node.js 20.19+ or 22.12+ (https://nodejs.org)")
    npm = shutil.which("npm")
    if not npm:
        problems.append("npm (ships with Node.js)")
    if problems:
        log("Missing requirements:\n  - " + "\n  - ".join(problems))
        return 1
    cargo_version = version([cargo, "--version"])
    log(f"OK  Python {sys.version.split()[0]}, Node {'.'.join(map(str, node))}, cargo {'.'.join(map(str, cargo_version or ()))}")

    log("Installing frontend dependencies (npm ci)...")
    subprocess.run([npm, "ci"], cwd=ROOT / "frontend", check=True)

    env = ROOT / "backend" / ".env"
    if not env.exists():
        shutil.copy(ROOT / "backend" / ".env.example", env)
        log("Created backend/.env - add API keys there to enable live vessels and road traffic.")

    if not args.skip_data:
        log("Importing airports, seaports and nuclear plants...")
        result = subprocess.run([sys.executable, str(ROOT / "scripts" / "ingest.py")])
        if result.returncode:
            log("Some datasets failed to import; run `python scripts/ingest.py` again later.")

    log("\nSetup complete. Start the app with:  python scripts/dev.py   (or: make dev)")
    log("Optional network layers:            python scripts/build_tiles.py --help")
    return 0


if __name__ == "__main__":
    sys.exit(main())
