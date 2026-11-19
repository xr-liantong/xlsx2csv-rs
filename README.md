# xlsx2csv-rs

A Rust port of [dilshod/xlsx2csv](https://github.com/dilshod/xlsx2csv) 0.8.6
(anchor commit `ec1883bb438ee4baf05953695009b9b6e9cad966`, MIT).

The port reproduces the upstream command line and its CSV output **byte for
byte** on every fixture we track; differences are listed in
[KNOWN-GAPS.md](KNOWN-GAPS.md). No upstream bug is copied on purpose: the
acceptance target is what upstream 0.8.6 actually prints on the fixtures.

## Status: v0.1.0

Parity — 21/21 golden cases match the upstream output byte for byte
(`cmp`, exit 0):

| fixture | arguments | bytes |
|---|---|---|
| f1-basic | (none) | 418 |
| f1-basic | `-d ';'` | 418 |
| f2-dates | (none) | 240 |
| f3-floats | (none) | 209 |
| f4-sheets | (none) | 8 |
| f4-sheets | `-a` | 86 |
| f4-sheets | `-s 2` | 4 |
| f4-sheets | `-n 'S2 名称'` | 4 |
| f5-quirks | (none) | 2095 |
| f6-big | (none) | 3,355,221 |
| f7-hyperlinks | `--hyperlinks` | 402 |
| f7-hyperlinks | (none) | 94 |
| f4-sheets | `-a -I '^S2'` | 27 |
| f4-sheets | `-a -I S1 S2` | 51 |
| f4-sheets | `-a -E '^S'` | 35 |
| f8-merged | `-m` | 26 |
| f8-merged | (none) | 17 |
| f5-quirks | `--no-line-breaks` | 2093 |
| f1-basic | `--no-line-breaks` | 414 |
| f9-shared | (none) | 73 |
| f9-shared | `-e` | 74 |
| f1-basic | `--no-line-breaks` | 414 |

Gate results (`cargo fmt --check`, `cargo clippy --release --all-targets --
-D warnings`, `cargo test --release`): all exit 0.

## Results

Box: Ubuntu 24.04, 2 vCPU, 3.8 GB RAM. Three runs each, median reported.
Both scales are byte-identical to upstream output (`cmp` clean) — the two
tables are **different workloads**, do not compare one row against the other
table's row.

### 50,000 rows x 8 columns (`f6-big`, output 3,355,221 B)

| implementation | median wall (3 runs) | peak RSS |
|---|---|---|
| xlsx2csv-rs (release) | 512 ms (509 / 512 / 539) | 20,592 KB |
| upstream, CPython 3.12 | 2,701 ms | 24,220 KB |

### 100,000 rows x 8 columns (`f7-big100k`, 4.4 MB xlsx, output 6,510,106 B)

| implementation | median wall (3 runs) | peak RSS |
|---|---|---|
| xlsx2csv-rs (release) | 803 ms (643 / 803 / 841) | 42,624 KB |
| upstream, CPython 3.12 | 3,136 ms (3,633 / 3,056 / 3,137) | 24,112 KB |

Scaling 50k -> 100k is roughly linear in time (0.51 s -> 0.80 s) and
sub-linear in wall for upstream (2.70 s -> 3.14 s per run spread), while
peak RSS grows with the sheet: at 100k rows this port uses ~1.8x upstream's
memory because it keeps the shared-string table and the parsed style table
resident. Speedup is 5.3x at 50k and 3.9x at 100k.
Numbers were measured on the box, not on a laptop.

## Usage

```
xlsx2csv-rs [options] xlsxfile [outfile]
```

Supported today: `-a/--all`, `-d/--delimiter`, `-s/--sheet`,
`-n/--sheetname`, `-i/--ignoreempty`, `-e/--escape`, `-E`/`-I` sheet-name
patterns, `-m/--merge-cells`, `--hyperlinks`, `--no-line-breaks`,
`--skipemptycolumns`, `-f/--dateformat`, `-t/--timeformat`,
`--floatformat`, `--sci-float`, `--ignore-percentage`, `--ignore-formats`,
`--exclude_hidden_sheets`, `--include-hidden-rows`, `-l/--lineterminator`,
`-p/--sheetdelimiter`, `-q/--quoting`, `-h/--help`, `-v/--version`.

A directory as input converts every file inside it to `<name>csv` next to the
source (or into the outfile directory); `--continue-on-error` keeps walking
after a file that is not a zip.

Exit codes: `0` success, `1` runtime failure, `2` usage error.

## Build and test

```
cargo build --release
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

### Fixtures and parity

The `.xlsx` inputs are generated, not tracked, so a clean clone needs one
generation step before the end-to-end tests can compare anything:

```
python3 -m pip install openpyxl          # only dependency of the generator
python3 fixtures/gen_fixtures.py         # writes fixtures/*.xlsx, reproducible
tools/parity.sh                          # 32 cases against fixtures/*.golden.csv
```

The 32 cases are 20 `check` (candidate binary, expected golden), 9
`check_file_first` (input path before a `nargs='+'` option, golden tracked)
and 3 `check_enc` (`-c` output encodings). `cargo test` pins 15 of them as
golden cases and fails — instead of skipping — when a fixture is missing. `f6-big`'s golden is not tracked, so
a clean clone reports 31 passed and 1 skipped; see `ALLOW_SKIP` below.

`gen_fixtures.py` takes an optional output directory (`python3
fixtures/gen_fixtures.py /tmp/fx`) and produces byte-identical workbooks on
every run: `docProps/core.xml` and the zip entry timestamps are pinned, so two
runs hash the same. `tools/parity.sh` takes a binary and a fixtures directory as
optional arguments (`tools/parity.sh /tmp/fx/xlsx2csv-rs /tmp/fx`) and prints
`N passed, M failed, K skipped, L unexpected skip(s)`.

A case skips when its input or golden is absent, but only cases named in
`ALLOW_SKIP` may do so quietly. `f6-big.golden.csv` is 3.3 MB and stays out of
git by design, so `f6-big` is the whole allowlist:

```
$ git archive HEAD | tar -x -C /tmp/c1 && cd /tmp/c1
$ python3 fixtures/gen_fixtures.py && bash tools/parity.sh; echo rc=$?
parity: 31 passed, 0 failed, 1 skipped, 0 unexpected skip(s)
rc=0

$ mv fixtures/f8-merged.xlsx /tmp/ && bash tools/parity.sh; echo rc=$?
parity: 27 passed, 0 failed, 1 skipped, 2 unexpected skip(s)
parity: unexpected skip -- a fixture or golden is missing (allowed: f6-big)
rc=2
```

Any other skip is a lost fixture or golden, and the run exits 2 instead of 0,
so a silently shrinking suite cannot pass CI. `ALLOW_SKIP="f6-big other-case"`
widens the allowlist when a second large golden is added on purpose.

## Layout

```
src/cli.rs      argument parsing (argparse-compatible exits)
src/port.rs     package -> workbook -> sheet -> CSV driver
src/ooxml.rs    zip package, [Content_Types].xml, rels, workbook
src/sheet.rs    streaming worksheet reader (rows, cells, spans)
src/styles.rs   numFmt / cellXfs lookup, standard format table
src/sst.rs      shared strings
src/fmtval.rs   number / date / percentage formatting
src/dates.rs    Excel serial -> date, format code translation
src/csvw.rs     csv.writer-compatible quoting
```

## Output encoding

`-c/--outputencoding` behaves like upstream: the codec is applied to the
output **file** only, because upstream passes `encoding=` to `open()` and
leaves `sys.stdout` untouched — so `-c latin-1` to stdout is a no-op there and
here. Supported codecs: `utf-8` (default), `ascii`, `latin-1`, `cp1252`,
`utf-16`, `utf-16-le`, `utf-16-be`. A character the codec cannot represent
fails with exit 1 and CPython's message shape:

```
'latin-1' codec can't encode character '\u4e2d' in position 2: ordinal not in range(256)
```

Anything else (gbk, shift_jis, ...) is rejected with `unknown encoding: <name>`
rather than silently writing UTF-8 under that name; see KNOWN-GAPS.md.

## Differential fuzz

Hand-picked fixtures only cover what we thought of. `tools/fuzz_parity.py`
builds random workbooks with openpyxl (1-3 sheets, hidden sheets and rows,
merged ranges, fourteen number formats, CJK and emoji, embedded tabs and
newlines, quotes) and runs both implementations under eighteen random flag
combinations (`-d`, `-l`, `-q`, `--floatformat`, `-e`, `--no-line-breaks`,
`-m`, `-i`, `--skipemptycolumns`, `--ignore-percentage`, `--sci-float`, `-f`,
`-t`), comparing stdout bytes, stderr and exit codes.

```sh
python3 tools/fuzz_parity.py --cases 40 --seed 7
# fuzz: 40 ok, 0 FAILED
```

It found the four bugs fixed in this release line and keeps them from coming
back: negative serial fractions wrapping through `u32` in time formats,
`--floatformat %.3f` ignoring its precision, `-q nonnumeric` quoting nearly
nothing (CPython only exempts real `int`/`float` objects, and this port always
hands the writer strings), and out-of-range serials in date formats.
`--only <seed>` replays one case, `--keep-failures` leaves the failing pair in
`fixtures/`.

## License

MIT, see [LICENSE](LICENSE). Upstream LICENSE: MIT per PyPI metadata; the
upstream repository ships no LICENSE file, so `LICENSE` here restates MIT and
credits Dilshod Tadjibaev and contributors.
