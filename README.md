# xlsx2csv-rs

A Rust port of [dilshod/xlsx2csv](https://github.com/dilshod/xlsx2csv) 0.8.6
(anchor commit `ec1883bb438ee4baf05953695009b9b6e9cad966`, MIT).

The port reproduces the upstream command line and its CSV output **byte for
byte** on every fixture we track; differences are listed in
[KNOWN-GAPS.md](KNOWN-GAPS.md). No upstream bug is copied on purpose: the
acceptance target is what upstream 0.8.6 actually prints on the fixtures.

## Status: v0.1.0

Parity — 10/10 golden cases match the upstream output byte for byte
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

Gate results (`cargo fmt --check`, `cargo clippy --release --all-targets --
-D warnings`, `cargo test --release`): all exit 0.

## Results

Box: Ubuntu 24.04, 2 vCPU, 3.8 GB RAM. Three runs each, median reported.
Both scales are byte-identical to upstream output (`cmp` clean). The two
tables below are **different workloads** — read a row against its own table
only.

### 50,000 rows x 8 columns (`f6-big`, xlsx 3.0 MB, output 3,355,221 B)

| implementation | median wall (3 runs) | peak RSS |
|---|---|---|
| xlsx2csv-rs (release) | 512 ms (509 / 512 / 539) | 20,592 KB |
| upstream, CPython 3.12 | 2,701 ms | 24,220 KB |

### 100,000 rows x 8 columns (`f7-big100k`, xlsx 4.4 MB, output 6,510,106 B)

| implementation | median wall (3 runs) | peak RSS |
|---|---|---|
| xlsx2csv-rs (release) | 803 ms (643 / 803 / 841) | 42,624 KB |
| upstream, CPython 3.12 | 3,136 ms (3,633 / 3,056 / 3,137) | 24,112 KB |

Speedup is 5.3x at 50k rows and 3.9x at 100k rows. Wall time scales roughly
with the row count (0.51 s -> 0.80 s for 2x the rows). Peak RSS grows with the
sheet: at 100k rows this port uses ~1.8x upstream's memory, because it keeps
the shared-string table and the parsed style table resident for the whole
sheet, while upstream streams them per cell. The 50k row comes from an
earlier session on the same box; the 100k row was measured in one session
(three runs each, interleaved).

## Usage

```
xlsx2csv-rs [options] xlsxfile [outfile]
```

Supported today: `-a/--all`, `-d/--delimiter`, `-s/--sheet`,
`-n/--sheetname`, `-i/--ignoreempty`, `-e/--escape`,
`--skipemptycolumns`, `-f/--dateformat`, `-t/--timeformat`,
`--floatformat`, `--sci-float`, `--ignore-percentage`, `--ignore-formats`,
`--exclude_hidden_sheets`, `--include-hidden-rows`, `-l/--lineterminator`,
`-p/--sheetdelimiter`, `-q/--quoting`, `-h/--help`, `-v/--version`.

Exit codes: `0` success, `1` runtime failure, `2` usage error.

## Build and test

```
cargo build --release
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

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

## License

MIT, see [LICENSE](LICENSE). Upstream is MIT as well.
