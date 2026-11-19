#!/usr/bin/env python3
"""Differential fuzz: random workbook + random flags, upstream vs xlsx2csv-rs.

    python3 tools/fuzz_parity.py --cases 40 --seed 7 [--keep-failures]

Every case builds a workbook from a seed, runs both binaries with the same
arguments, and compares stdout, stderr and exit code byte for byte. The first
mismatch is reported with its seed and the exact command so it can be
reproduced with `--only <seed>`; failing workbooks are kept under
`/tmp/fuzz-keep/<seed>/` when `--keep-failures` is set.
"""
import argparse
import os
import random
import shutil
import subprocess
import sys
import tempfile
from openpyxl import Workbook
from openpyxl.styles import numbers
from openpyxl.utils import get_column_letter

UPSTREAM = ["python3", "upstream/xlsx2csv.py"]
OURS = ["./target/release/xlsx2csv-rs"]
INLINE = ('<c r="{ref}" t="inlineStr"><is><t>{text}</t></is></c>',)

FORMATS = [
    "General", "0", "0.00", "0.0", "#,##0", "0%", "0.00%", "0.00E+00",
    "yyyy-mm-dd", "yyyy/m/d hh:mm", "hh:mm:ss", "m/d/yy", "@", "# ?/?",
]
WORDS = ["alpha", "中文", "emoji 🚀", "a,b", 'q"q', " lead", "trail ",
         "tab\there", "line\nbreak", "", "0", "1e5", "NULL", "-1", "3.5"]
FLAGS = [
    [], ["-d", ";"], ["-d", "tab"], ["-l", "\\r\\n"], ["-q", "all"],
    ["-q", "nonnumeric"], ["-q", "none"], ["--floatformat", "%f"],
    ["--floatformat", "%.3f"], ["-e"], ["--no-line-breaks"], ["-m"],
    ["-i"], ["--skipemptycolumns"], ["--ignore-percentage"],
    ["--sci-float"], ["-f", "%Y/%m/%d"], ["-t", "%H:%M:%S"],
]

def build(path, rng):
    """A workbook that leans on the corners the port has to get right."""
    wb = Workbook()
    hidden_rows = rng.random() < 0.3
    for index in range(rng.randint(1, 3)):
        ws = wb.active if index == 0 else wb.create_sheet()
        ws.title = rng.choice(["S", "S2 名称", "hidden", "Data 1"]) + str(index)
        rows, cols = rng.randint(1, 8), rng.randint(1, 5)
        for row in range(1, rows + 1):
            for col in range(1, cols + 1):
                cell = ws.cell(row=row, column=col)
                kind = rng.random()
                if kind < 0.30:
                    cell.value = rng.choice(WORDS) + str(rng.randint(0, 9))
                elif kind < 0.55:
                    cell.value = rng.choice([0, 1, -1, 42, 2 ** 40, -0.5])
                elif kind < 0.85:
                    cell.value = rng.uniform(-1e4, 1e4) / rng.choice([1, 7, 1000])
                else:
                    cell.value = rng.choice([0.0, 0.9995, 1e-7, 2.675, -0.135])
                cell.number_format = rng.choice(FORMATS)
        if rng.random() < 0.4:
            col = get_column_letter(rng.randint(1, cols))
            ws.column_dimensions[col].hidden = True
        if hidden_rows and rows > 2:
            ws.row_dimensions[rng.randint(1, rows)].hidden = True
        if rng.random() < 0.3 and rows > 1:
            ws.merge_cells(f"A1:{get_column_letter(min(cols, 2))}1")
    # A workbook must keep one visible sheet; hide at most n-1 of them.
    if len(wb.worksheets) > 1 and rng.random() < 0.5:
        for sheet in rng.sample(wb.worksheets, rng.randint(1, len(wb.worksheets) - 1)):
            sheet.sheet_state = "hidden"
    wb.save(path)

def run(cmd, path, args):
    full = cmd + args + [path]
    done = subprocess.run(full, capture_output=True)
    return done.returncode, done.stdout, done.stderr, " ".join(full[2:])

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cases", type=int, default=40)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--only", type=int, default=None)
    parser.add_argument("--keep-failures", action="store_true")
    opts = parser.parse_args()
    seeds = [opts.only] if opts.only is not None else list(range(opts.seed, opts.seed + opts.cases))
    failures = 0
    for seed in seeds:
        rng = random.Random(seed)
        workdir = tempfile.mkdtemp(prefix="fuzz-")
        path = os.path.join(workdir, "case.xlsx")
        build(path, rng)
        args = rng.choice(FLAGS)
        if rng.random() < 0.3:
            args = list(args) + ["--skipemptycolumns"]
        want = run(UPSTREAM, path, args)
        got = run(OURS, path, args)
        # Upstream raises an uncaught OverflowError for out-of-range date
        # serials: the traceback text is unreproducible by design (see
        # KNOWN-GAPS.md), so those cases compare exit code and stdout only.
        traceback = b"Traceback (most recent call last)" in want[2]
        stderr_same = traceback or want[2].strip() == got[2].strip()
        same = want[0] == got[0] and want[1] == got[1] and stderr_same
        if same:
            note = " (upstream traceback: rc+stdout only)" if traceback else ""
            print(f"ok   seed={seed} {len(want[1])} B{note} {want[3]}")
            shutil.rmtree(workdir)
            continue
        failures += 1
        print(f"FAIL seed={seed} rc {want[0]}/{got[0]} bytes {len(want[1])}/{len(got[1])}")
        print(f"     cmd: {want[3]}")
        at = next((i for i, (a, b) in enumerate(zip(want[1], got[1])) if a != b), None)
        print(f"     first byte diff: {at}")
        if at is not None:
            print(f"     want: {want[1][max(0, at-40):at+40]!r}")
            print(f"     got : {got[1][max(0, at-40):at+40]!r}")
        if want[2].strip() != got[2].strip():
            print(f"     stderr want: {want[2][:200]!r}")
            print(f"     stderr got : {got[2][:200]!r}")
        if opts.keep_failures:
            keep = f"/tmp/fuzz-keep/{seed}"
            shutil.copytree(workdir, keep, dirs_exist_ok=True)
            print(f"     kept: {keep}")
        shutil.rmtree(workdir)
    print(f"fuzz: {len(seeds) - failures} ok, {failures} FAILED")
    return 1 if failures else 0

if __name__ == "__main__":
    sys.exit(main())
