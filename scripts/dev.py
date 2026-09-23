#!/usr/bin/env python3
"""Run the backend (cargo run) and the frontend dev server (Vite) together.

    python scripts/dev.py

Open http://localhost:5173. Press Ctrl+C to stop both.
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import threading
import time

from common import ROOT, log


def tool(name: str) -> str:
    path = shutil.which(name)
    if not path and name == "cargo":
        candidate = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo")
        path = shutil.which(candidate)
    if not path:
        sys.exit(f"'{name}' was not found on PATH. Run `python scripts/setup.py` for requirements.")
    return path


def pump(process: subprocess.Popen, prefix: str) -> None:
    assert process.stdout
    for line in process.stdout:
        sys.stdout.write(f"[{prefix}] {line}")
        sys.stdout.flush()


def stop(process: subprocess.Popen) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        # npm.cmd starts node as a child; kill the whole tree.
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(process.pid)], capture_output=True)
    else:
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()


def main() -> int:
    cargo, npm = tool("cargo"), tool("npm")
    frontend = ROOT / "frontend"
    if not (frontend / "node_modules").exists():
        log("Installing frontend dependencies...")
        subprocess.run([npm, "ci"], cwd=frontend, check=True)

    options = dict(stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace", bufsize=1)
    processes = [
        ("backend", subprocess.Popen([cargo, "run"], cwd=ROOT / "backend", **options)),
        ("frontend", subprocess.Popen([npm, "run", "dev"], cwd=frontend, **options)),
    ]
    for name, process in processes:
        threading.Thread(target=pump, args=(process, name), daemon=True).start()
    log("Backend on http://127.0.0.1:3000, app on http://localhost:5173 (Ctrl+C stops both)")
    try:
        while all(p.poll() is None for _, p in processes):
            time.sleep(0.5)
        for name, p in processes:
            if p.poll() is not None:
                log(f"{name} exited with code {p.returncode}")
    except KeyboardInterrupt:
        log("Stopping...")
    finally:
        for _, p in processes:
            stop(p)
    return 0


if __name__ == "__main__":
    sys.exit(main())
