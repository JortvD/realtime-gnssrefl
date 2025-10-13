#!/usr/bin/env python3
"""
Reads stdout from `cargo run --release` (or any provided cargo args),
extracts messages of the form

  SECTOR:<data>
  MEASUREMENT:<data>
  OBSERVATION:<data>
  DATA:<data>

and writes each to its own CSV file (sector.csv, measurement.csv,
observation.csv, data.csv) inside an output directory.

Because defmt may add noisy prefixes/suffixes like
  [INFO] SECTOR:1,2,3 (<crate> └─ <invalid location: defmt frame-index: 23>:0)
we robustly strip typical trailing decorations after the payload.

Usage:
  python dump.py               # runs `cargo run --release`
  python dump.py --out logs    # choose output directory
  python dump.py -- cargo run -q --release  # pass custom cargo command after "--"

Press Ctrl+C to stop. Files are opened in append mode.
"""
from __future__ import annotations

import argparse
import csv
import os
import re
import signal
import subprocess as sp
import sys
from pathlib import Path
from typing import Dict, IO, Iterable, List

# Regex to find TYPE:payload anywhere in a line, despite defmt noise.
TYPE_PATTERN = re.compile(r"\b(SECTOR|MEASUREMENT|OBSERVATION|DATA)\s*:\s*([^\r\n]+)")

# Heuristics to trim typical defmt suffixes after the payload
# e.g., " (<crate> └─ <invalid location: defmt frame-index: 23>:0)"
TRAILING_NOISE_PATTERNS = [
    re.compile(r"\s*\(<[^>]*>.*$"),  # anything starting with " (<...>"
    re.compile(r"\s*\(defmt:[^)]*\).*$"),  # parenthetical defmt notes
]


def clean_payload(payload: str) -> str:
    s = payload.strip()
    for pat in TRAILING_NOISE_PATTERNS:
        s = pat.sub("", s)
    # Extra conservative trim when defmt writes stack-ish junk
    if " (<" in s:
        s = s.split(" (<", 1)[0].rstrip()
    return s


def parse_rows(payload: str) -> List[List[str]]:
    """Parse CSV-like payload into rows.
    Accepts a single logical CSV row (most common) but gracefully
    handles cases where device emits semicolon-separated segments by
    splitting on \n or ; if needed.
    """
    payload = payload.strip()
    if not payload:
        return []

    # If payload contains explicit newlines, split and parse each.
    candidates: Iterable[str] = payload.splitlines()

    rows: List[List[str]] = []
    for c in candidates:
        c = c.strip()
        if not c:
            continue
        # Some firmwares separate groups with semicolons; split conservatively
        parts = [c]
        if ";" in c and "," not in c:
            parts = [p.strip() for p in c.split(";") if p.strip()]
        for p in parts:
            try:
                # Use Python's CSV reader to respect quotes if present
                for row in csv.reader([p]):
                    rows.append([cell.strip() for cell in row])
            except Exception:
                rows.append([p])
    return rows


def ensure_writers(outdir: Path) -> Dict[str, csv.writer]:
    outdir.mkdir(parents=True, exist_ok=True)
    files: Dict[str, IO[str]] = {}
    writers: Dict[str, csv.writer] = {}
    # Map type -> filename
    mapping = {
        "SECTOR": outdir / "sector.csv",
        "MEASUREMENT": outdir / "measurement.csv",
        "OBSERVATION": outdir / "observation.csv",
        "DATA": outdir / "data.csv",
    }
    # Open append mode (newline="") so csv module controls newlines
    for key, path in mapping.items():
        f = open(path, "w", newline="", encoding="utf-8")
        files[key] = f
        writers[key] = csv.writer(f)
    # Stash file handles on the dict so we can close later
    writers["__files__"] = files  # type: ignore
    return writers


def close_writers(writers: Dict[str, csv.writer]) -> None:
    files = writers.get("__files__")  # type: ignore
    if isinstance(files, dict):
        for f in files.values():
            try:
                f.flush()
                f.close()
            except Exception:
                pass


def stream_and_capture(cmd: List[str], outdir: Path) -> int:
    writers = ensure_writers(outdir)

    # Ensure child is killed on Ctrl+C on POSIX; on Windows, we'll terminate in finally
    proc = sp.Popen(
        cmd,
        stdout=sp.PIPE,
        stderr=sp.STDOUT,
        bufsize=1,
        text=True,
        encoding="utf-8",
        errors="replace",
    )

    def handle_sigint(signum, frame):
        try:
            proc.terminate()
        except Exception:
            pass
    try:
        signal.signal(signal.SIGINT, handle_sigint)
    except Exception:
        # Some environments may not allow signal override; carry on
        pass

    try:
        assert proc.stdout is not None
        for line in proc.stdout:
            if line.startswith("[INFO ] ["):
                sys.stdout.write(line)
                sys.stdout.flush()

            for m in TYPE_PATTERN.finditer(line):
                mtype = m.group(1)
                payload = clean_payload(m.group(2))
                rows = parse_rows(payload)
                if not rows:
                    continue
                w = writers.get(mtype)
                if not w:
                    continue
                for row in rows:
                    w.writerow(row)
                # Flush promptly to avoid data loss on crashes
                fw = writers.get("__files__")  # type: ignore
                if isinstance(fw, dict):
                    fh = fw.get(mtype)
                    if fh:
                        fh.flush()
        proc.wait()
        return proc.returncode or 0
    finally:
        try:
            if proc.poll() is None:
                proc.terminate()
        except Exception:
            pass
        close_writers(writers)


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run cargo and split device logs into CSVs by type.")
    parser.add_argument("--out", default="logs", help="Output directory for CSV files (default: logs)")
    parser.add_argument(
        "--",
        dest="dashdash",
        nargs=argparse.REMAINDER,
        help="Everything after -- is the exact command to run (default: cargo run --release)",
    )
    args = parser.parse_args(argv)

    outdir = Path(args.out)
    cmd = ["cargo", "run", "--release"]
    if args.dashdash:
        # Allow: python pico_logger.py -- cargo run -q --release
        cmd = args.dashdash

    print(f"Running: {' '.join(cmd)}\nWriting CSVs to: {outdir.resolve()}\nPress Ctrl+C to stop.\n", file=sys.stderr)

    try:
        return stream_and_capture(cmd, outdir)
    except FileNotFoundError as e:
        print(f"Error: {e}. Is Cargo installed and on PATH?", file=sys.stderr)
        return 127
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
