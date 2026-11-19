# Known gaps and divergences (v0.1.0)

Acceptance for v0.1.0: byte-identical output to upstream 0.8.6 on the ten
golden cases in `fixtures/`. Everything below is outside that net, either
because it is unimplemented or because no fixture covers it yet.

## Not implemented

Nothing: every upstream option is implemented; `-c` is weaker (see below).


`--hyperlinks` is implemented and covered by `fixtures/f7-hyperlinks.xlsx`
(single-cell refs, an `A6:A7` range ref, unicode target, empty cell, mailto,
date/percent/number cells). Its output stays byte-identical to upstream,
including the quirk that a linked cell with no text prints the URL as value.

`-E` / `-I` are implemented for the `re` subset we can honour: literals, `.`,
`[...]` with ranges, `*`, `+`, `?`, `^`, `$`, `\x`. Patterns using groups,
alternation or `{m,n}` are rejected with exit 1 and a message naming the
pattern, instead of silently selecting different sheets. `-E`/`-I` only filter
the all-sheets mode (`-a`), exactly like upstream, and take their values before
the input path because they are `nargs='+'`.

`--no-line-breaks` is implemented: `\r`, `\n` and `\t` inside a cell value
become spaces, after merge substitution and hyperlink wrapping, as upstream
does it.

`-m` / `--merge-cells` is implemented: every cell of a `ref="A1:B2"` range takes
the top-left cell's already-wrapped value, exactly like upstream. Unlike
upstream, a range whose followers appear *before* the corner exits 1 with
`merged cell B2 precedes its corner A2` instead of a Python `KeyError`
traceback; a range with a single-cell ref is ignored, as upstream does.

A CR (`\r`) inside a field is always quoted, like CPython's `_csv`, even when
the terminator does not contain it; `fixtures/f9-shared.xlsx` covers that.

## Accepted but weaker than upstream

| option | state |
|---|---|
| `-c`, `--outputencoding` | parsed, output is always UTF-8 |

Directory input is implemented (`convert_recursive`): every file in the tree
becomes `<name>csv` next to itself (upstream's `[:-4] + "csv"` naming, so a
`README.txt` becomes `READMEcsv`), `--continue-on-error` walks past a file that
is not a zip, and an outfile directory collects the results. Upstream lists
entries in filesystem order, so the order of the `Converting ... to ...` lines
is not part of the contract.

## `-e` / `--escape`

Two paths, as upstream has: the shared-string table is escaped once before any
sheet is written, and `<v>` bodies get `\\r` / `\\n` / `\\t` for embedded
CR/LF/TAB. Inline strings (`t="inlineStr"`, what openpyxl 3.1.5 writes) are not
escaped, which is also upstream's behaviour -- `fixtures/f9-shared.xlsx` carries
a real sharedStrings part so the escaping path is actually exercised.

One deliberate difference: upstream rewrites only the XML entities `&#13;`,
`&#10;`, `&#9;` inside `<v>`; we escape the decoded characters, so a raw CR in
a `<v>` body is escaped too. A `<v>` body only holds numbers in practice.

## Known divergences

None observed on the tracked fixtures.

Two upstream quirks are reproduced on purpose, because the acceptance target
is upstream's real output:

* `[Content_Types].xml` with several worksheet overrides: only the last one
  is kept (upstream keeps the last assignment, not the first).
* `-i` (`--ignoreempty`) does not advance the "last row number" counter
  upstream uses to pad blank rows.

## Fixtures are not reproducible yet

The six `.xlsx` fixtures are produced by `fixtures/gen_fixtures.py`
(openpyxl 3.1.5), which is tracked here, but its output bytes differ run to
run: openpyxl stamps `docProps/core.xml` with `created`/`modified`, and zip
local headers carry their own timestamps. The CSV goldens are stable; the
input bytes are not. Tracked as issue #1. Until that is fixed,
`fixtures/*.xlsx` is git-ignored and the goldens in `fixtures/*.golden.csv`
are the reference (they were produced by upstream 0.8.6 itself).

## CI is not wired up

There is no CI file in the repository: the `x-access-token` this project is
published with has no `workflow` scope (GitHub answers `refusing to allow an
OAuth App to create or update workflow`), so a committed workflow block would
make every push fail. CI 待接入，需 workflow scope，暂由 box 人工门禁替代：every
release is gated by the four commands in README's Gate section, run on the box
against the exact tree that is pushed.

## Out-of-range serials in date formats

A serial far outside the calendar (for example `1099511627776` under `m/d/yy`)
makes upstream `datetime.timedelta` raise an uncaught `OverflowError`: exit 1,
stdout truncated right after the last good cell, and a Python traceback on
stderr. This port matches the exit code and the truncated stdout byte for byte,
but writes one line instead of a traceback:

```
Error: potential invalid date format.
```

The traceback text is not worth reproducing; scripts that grep stderr for
`OverflowError` should look at the exit code instead.

## Fuzz coverage

`tools/fuzz_parity.py` compares stdout, stderr and exit code, but only for the
flag combinations it samples. Two cases are deliberately excluded from full
comparison: upstream tracebacks (exit code and stdout only) and workbooks it
cannot build (skipped). The sampled flags do not include `-c`, `-E`/`-I`,
`--ignore-formats` or directory input; those are covered by fixtures instead.

## Output encoding coverage

`-c/--outputencoding` covers `utf-8`, `ascii`, `latin-1`, `cp1252`, `utf-16`,
`utf-16-le` and `utf-16-be` — the codecs that need no dependency. Every other
CPython codec name (`gbk`, `big5`, `shift_jis`, `koi8-r`, ...) is rejected with
`unknown encoding: <name>` instead of being accepted and silently written as
UTF-8. Upstream would accept them.

Two smaller differences in the failure path: upstream raises an uncaught
`UnicodeEncodeError` (traceback on stderr) and stops mid-file; this port writes
the same partial bytes, exits 1, and prints one line. Upstream reports a range
of positions (`in position 2-3`) when a row has several unencodable characters;
this port reports the first one.

## argparse text on stderr (#5)

`usage`/`error` lines are now byte-identical to argparse, including the wrapped
usage block, the `prog: error: ...` prefix and the extra blank line that
`sys.exit(str(e) + "\n")` leaves. Two deliberate differences remain:

- The program name is `argv[0]`'s basename, as argparse does. Upstream prints
  `xlsx2csv.py` because that is how it is invoked; comparing bytes therefore
  needs the same binary name (`cp target/release/xlsx2csv-rs /tmp/xlsx2csv.py`).
- `-d ''` crashes upstream (`IndexError: string index out of range`, exit 1,
  traceback on stderr). We report `error: invalid delimiter` with exit 1 and no
  output; the exit code and stdout match, the traceback does not.

## XML end-of-line handling (#11, fixed)

A *literal* `\r` (0x0D) in a text node is normalised to `\n` (XML 1.0 §2.11)
before entities are decoded, so `&#13;` still yields a real CR — same split as
expat/upstream. Covered by `fixtures/f11-literal-cr.xlsx` (literal CR) and
`fixtures/f9-shared.xlsx` (character reference), both with and without `-e`.

## Directory input snapshots its listing (#8, fixed)

`convert_recursive` walks `os.listdir(path)`, a snapshot taken before any output
is written. The Rust walk now collects the directory entries first for the same
reason: on a lazy `read_dir` the `.csv` just written next to its `.xlsx` came
back as an input on the next `getdents` batch and the run died with
`Invalid xlsx file: <dir>/a.csv` (rc 1 instead of upstream's rc 0).
A *second* run over the same tree still fails on the produced `.csv`, exactly
like upstream.

## Integration test coverage (#6, fixed)

`tests/parity.rs` now pins 15 golden cases (`f1`, `f1 -d ;`, `f2`, `f3`, `f4`,
`f4 -s 2`, `f4 -n`, `f5 --no-line-breaks`, `f6-big`, `f7` ×2, `f8` ×2,
`f9`/`f11` ×2, `f10` ×3, encodings ×3). A **missing fixture no longer skips
silently**: the test panics with the `gen_fixtures.py` hint unless
`XLSX2CSV_SKIP_MISSING_FIXTURES=1` is set for a local run without Python.
The only tolerated absence is `fixtures/f6-big.golden.csv` (3.3 MB, untracked),
which mirrors `ALLOW_SKIP=f6-big` in `tools/parity.sh`.
