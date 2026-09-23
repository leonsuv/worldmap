"""Shared helpers for WorldMap data scripts (standard library only)."""

from __future__ import annotations

import gzip
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable, Iterable, Iterator

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = Path(os.environ.get("DATA_DIR") or ROOT / "data").resolve()
SOURCES_DIR = DATA_DIR / "sources"
TILES_DIR = DATA_DIR / "tiles"
USER_AGENT = "WorldMap-data/0.2 (+https://github.com/leonsuv/worldmap)"
OVERPASS_URLS = [
    os.environ.get("OVERPASS_URL", "https://overpass-api.de/api/interpreter"),
    "https://overpass.private.coffee/api/interpreter",
]

# Make console output work on Windows code pages.
for stream in (sys.stdout, sys.stderr):
    try:
        stream.reconfigure(errors="replace")  # type: ignore[attr-defined]
    except (AttributeError, ValueError):
        pass


def log(message: str) -> None:
    print(message, flush=True)


def fetch(url: str, *, data: bytes | None = None, timeout: int = 120, retries: int = 4, headers: dict | None = None) -> bytes:
    """GET (or POST when `data` is given) with retries and exponential backoff."""
    last: Exception | None = None
    for attempt in range(retries):
        request = urllib.request.Request(url, data=data, headers={"User-Agent": USER_AGENT, **(headers or {})})
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                body = response.read()
                if response.headers.get("Content-Encoding") == "gzip":
                    body = gzip.decompress(body)
                return body
        except urllib.error.HTTPError as error:
            last = error
            if error.code in (400, 401, 403, 404):
                raise
            wait = 60 if error.code == 429 else 5 * 2**attempt
        except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
            last = error
            wait = 5 * 2**attempt
        if attempt + 1 < retries:
            log(f"    retrying in {wait}s ({last})")
            time.sleep(wait)
    raise RuntimeError(f"download failed: {url} ({last})")


def download(url: str, target: Path, *, label: str | None = None) -> Path:
    """Stream a large file to disk with progress, resuming nothing but never leaving partial files."""
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = target.with_suffix(target.suffix + ".part")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    log(f"Downloading {label or url}")
    with urllib.request.urlopen(request, timeout=120) as response, open(partial, "wb") as out:
        total = int(response.headers.get("Content-Length") or 0)
        done = 0
        last_report = 0.0
        while chunk := response.read(1 << 20):
            out.write(chunk)
            done += len(chunk)
            if time.monotonic() - last_report > 2:
                last_report = time.monotonic()
                pct = f" ({done * 100 // total}%)" if total else ""
                log(f"    {done / 1_048_576:,.0f} MB{pct}")
    partial.replace(target)
    return target


def connect_static_db():
    import sqlite3

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DATA_DIR / "static.db", timeout=30)
    conn.execute("PRAGMA journal_mode=WAL")
    return conn


# ── Overpass ──────────────────────────────────────────────────────────────────

BBox = tuple[float, float, float, float]  # south, west, north, east


def world_cells(step: float = 30.0, bbox: BBox | None = None) -> list[BBox]:
    s0, w0, n0, e0 = bbox or (-60.0, -180.0, 84.0, 180.0)
    cells = []
    lat = s0
    while lat < n0:
        lon = w0
        top = min(lat + step, n0)
        while lon < e0:
            cells.append((lat, lon, top, min(lon + step, e0)))
            lon += step
        lat = top
    return cells


def split(cell: BBox) -> list[BBox]:
    s, w, n, e = cell
    mid_lat, mid_lon = (s + n) / 2, (w + e) / 2
    return [(s, w, mid_lat, mid_lon), (s, mid_lon, mid_lat, e), (mid_lat, w, n, mid_lon), (mid_lat, mid_lon, n, e)]


class OverpassOverload(Exception):
    """The query was too large for one request; split the area."""


def overpass(query: str, timeout: int) -> dict:
    last: Exception | None = None
    for attempt in range(6):
        url = OVERPASS_URLS[attempt % len(OVERPASS_URLS)] if attempt >= 3 else OVERPASS_URLS[0]
        try:
            body = fetch(url, data=urllib.parse.urlencode({"data": query}).encode(), timeout=timeout + 60, retries=1)
        except urllib.error.HTTPError as error:
            if error.code in (400,):
                raise
            last = error
            if error.code == 429:
                log("    Overpass busy (429); waiting 60 s")
                time.sleep(60)
                continue
            if error.code == 504:
                raise OverpassOverload(str(error)) from error
            time.sleep(15)
            continue
        except RuntimeError as error:
            last = error
            time.sleep(15)
            continue
        data = json.loads(body)
        remark = str(data.get("remark", ""))
        if "runtime error" in remark or "out of memory" in remark or "timed out" in remark:
            raise OverpassOverload(remark)
        return data
    raise RuntimeError(f"Overpass unavailable: {last}")


def overpass_cells(
    name: str,
    build_query: Callable[[BBox], str],
    *,
    bbox: BBox | None = None,
    step: float = 30.0,
    min_step: float = 1.0,
    timeout: int = 300,
) -> Iterator[dict]:
    """Yield Overpass elements for every cell, splitting cells that are too dense.

    Responses are cached per cell under data/sources/cache/<name>/, so an
    interrupted run resumes where it stopped.
    """
    cache = SOURCES_DIR / "cache" / name
    cache.mkdir(parents=True, exist_ok=True)
    pending = world_cells(step, bbox)
    done = 0
    while pending:
        cell = pending.pop(0)
        key = "_".join(f"{v:.4f}" for v in cell)
        cached = cache / f"{key}.json.gz"
        if cached.exists():
            elements = json.loads(gzip.decompress(cached.read_bytes()))
        else:
            size = cell[2] - cell[0]
            log(f"  Overpass {name}: cell {cell[0]:.1f},{cell[1]:.1f} ({size:g} deg) - {len(pending)} cells left")
            try:
                elements = overpass(build_query(cell), timeout).get("elements", [])
            except OverpassOverload as reason:
                if size / 2 < min_step:
                    log(f"    skipping dense cell {key}: {reason}")
                    continue
                log("    too dense, splitting into 4")
                pending[:0] = split(cell)
                continue
            cached.write_bytes(gzip.compress(json.dumps(elements).encode()))
            time.sleep(1.5)  # be polite to the shared public instance
        done += 1
        yield from elements
    log(f"  {done} cells processed")


def write_geojsonseq(path: Path, features: Iterable[dict]) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    partial = path.with_suffix(path.suffix + ".part")
    with open(partial, "w", encoding="utf-8") as out:
        for feature in features:
            out.write(json.dumps(feature, separators=(",", ":"), ensure_ascii=False))
            out.write("\n")
            count += 1
    partial.replace(path)
    return count


# ── Tile builder ──────────────────────────────────────────────────────────────

def backend_command() -> list[str]:
    """Command that runs the WorldMap backend binary (building it if needed)."""
    exe = "worldmap-backend.exe" if os.name == "nt" else "worldmap-backend"
    binary = ROOT / "backend" / "target" / "release" / exe
    cargo = shutil.which("cargo") or shutil.which(str(Path.home() / ".cargo" / "bin" / "cargo"))
    if cargo:
        # A no-op when the binary is current; rebuilds it after code changes.
        log("Preparing the tile builder (cargo build --release)...")
        result = subprocess.run([cargo, "build", "--release", "--manifest-path", str(ROOT / "backend" / "Cargo.toml")])
        if result.returncode != 0:
            if not binary.exists():
                sys.exit("Building the tile builder failed.")
            # On Windows a running server locks the executable; the existing build works too.
            log("Could not rebuild (is the WorldMap server running?); using the existing binary.")
    elif not binary.exists():
        sys.exit("Rust (cargo) is required to build tiles: https://rustup.rs")
    return [str(binary)]


def build_tiles(args: list[str]) -> None:
    command = backend_command() + ["tiles", "build", *args]
    log("Running: " + " ".join(command))
    subprocess.run(command, check=True)


def parse_bbox(value: str | None) -> BBox | None:
    """`west,south,east,north` → (south, west, north, east)."""
    if not value:
        return None
    w, s, e, n = (float(v) for v in value.split(","))
    if not (-180 <= w < e <= 180 and -90 <= s < n <= 90):
        raise ValueError("bbox must be west,south,east,north")
    return (s, w, n, e)
