#!/usr/bin/env python3
"""Runs macro_gen over the benchmark suite and tabulates the results.

Each benchmark is a directory benchmarks/<name>/ holding <name>.mlir and its
config <name>.toml. The runner runs every config with --emit-verilog and
--add-buffer <mode>, reads the report each run writes, and prints a summary table (also saved to benchmarks/build/summary.md).
Per design outputs and run logs go to benchmarks/build/<name>/.

Needs CIRCT_DIR set. Each run characterizes the reference inverter with
ngspice (about 40 s), so designs run in parallel (-j).

    benchmarks/run.py                     all benchmarks
    benchmarks/run.py c17 maj3            a subset
    benchmarks/run.py --mode non-invertible
"""

import argparse
import os
import subprocess
import sys
import tomllib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "benchmarks"


def discover() -> list[Path]:
    """Every benchmarks/<name>/<name>.toml, sorted by name."""
    return sorted(p / f"{p.name}.toml" for p in BENCH.iterdir() if (p / f"{p.name}.toml").exists())


def build(release: bool) -> Path:
    cmd = ["cargo", "build", "--features", "circt"] + (["--release"] if release else [])
    subprocess.run(cmd, cwd=ROOT, check=True)
    return ROOT / "target" / ("release" if release else "debug") / "macro_gen"


def run_one(binary: Path, config: Path, out: Path, mode: str) -> dict:
    name = config.stem
    out.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(binary), "--config", str(config), "--build-dir", str(out),
        "--emit-verilog", "--add-buffer", mode,
    ]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    log = proc.stdout + proc.stderr
    (out / "run.log").write_text(log)
    reports = list((out / "reports").glob("*.toml"))
    if proc.returncode != 0 or not reports:
        errors = [line for line in log.splitlines() if "ERROR" in line]
        reason = errors[-1].split("macro_gen]", 1)[-1].strip() if errors else f"exit {proc.returncode}"
        return {"name": name, "ok": False, "reason": reason}
    return {"name": name, "ok": True, "report": tomllib.loads(reports[0].read_text())}


HEADER = ["design", "status", "load", "outputs", "cells", "transistors", "logic stages",
          "F (worst)", "added inv", "stage effort f", "delay (τ)"]


def row(result: dict) -> list[str]:
    if not result["ok"]:
        return [result["name"], "FAIL", result["reason"]] + [""] * (len(HEADER) - 3)
    r = result["report"]
    ub, b = r["unbuffered"], r.get("buffered", r["unbuffered"])
    outs = r.get("outputs", [])
    worst = max(outs, key=lambda o: o["path_effort"]) if outs else None
    return [
        r["top"],
        "ok",
        f'{r["sizing"]["cload_cinv"]:g}',
        str(len(outs)),
        f'{ub["cells"]} → {b["cells"]}',
        f'{ub["transistors"]} → {b["transistors"]}',
        str(worst["logic_stages"]) if worst else "-",
        f'{worst["path_effort"]:.4g}' if worst else "-",
        str(sum(o["added_inverters"] for o in outs)),
        f'{ub["stage_effort"]:.2f} → {b["stage_effort"]:.2f}',
        f'{ub["delay_tau"]:.1f} → {b["delay_tau"]:.1f}',
    ]


def table(results: list[dict], mode: str) -> str:
    lines = [
        f"Mode: --add-buffer {mode}. Load in C_inv per output. Arrows read unbuffered → buffered. "
        "Delay is the logical effort estimate of the worst output (FO4 = 5 τ).",
        "",
        "| " + " | ".join(HEADER) + " |",
        "|" + "---|" * len(HEADER),
    ]
    lines += ["| " + " | ".join(row(r)) + " |" for r in results]
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("designs", nargs="*", help="benchmark names (default: all)")
    ap.add_argument("--mode", choices=["invertible", "non-invertible"], default="invertible")
    ap.add_argument("--out", type=Path, default=BENCH / "build")
    ap.add_argument("-j", "--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2),
                    help="benchmarks to run in parallel")
    ap.add_argument("--debug", action="store_true", help="use a debug build instead of --release")
    args = ap.parse_args()

    configs = discover()
    if args.designs:
        wanted = set(args.designs)
        configs = [c for c in configs if c.stem in wanted]
        missing = wanted - {c.stem for c in configs}
        if missing:
            ap.error(f"no such benchmark(s): {', '.join(sorted(missing))}")

    binary = build(release=not args.debug)

    def job(config: Path) -> dict:
        result = run_one(binary, config, args.out / config.stem, args.mode)
        print(f"  {config.stem}: {'ok' if result['ok'] else 'FAIL'}", flush=True)
        return result

    jobs = max(1, min(args.jobs, len(configs)))
    print(f"Running {len(configs)} benchmark(s), {jobs} at a time", flush=True)
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(job, configs))

    summary = table(results, args.mode)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "summary.md").write_text(summary)
    print("\n" + summary)
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
