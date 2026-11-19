# Known gaps and divergences (v0.1.0)

Acceptance for v0.1.0: byte-identical output to upstream 0.8.6 on the ten
golden cases in `fixtures/`. Everything below is outside that net, either
because it is unimplemented or because no fixture covers it yet.

## Not implemented (exit 1 with `not implemented yet`)

| option | upstream behaviour |
|---|---|
| `-m`, `--merge-cells` | emit merged ranges as extra rows |
| `--no-line-breaks` | strip newlines inside cell text |

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

## Accepted but weaker than upstream

| option | state |
|---|---|
| `-c`, `--outputencoding` | parsed, output is always UTF-8 |
| `-e`, `--escape` | parsed, `\r\n\t` escaping is not applied |
| directory as input | upstream converts every file recursively; we exit 1 |

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

`.github/workflows/ci.yml` exists in the working tree but cannot be pushed
with the credentials this project was published with (`x-access-token`, no
`workflow` scope; GitHub answers `refusing to allow an OAuth App to create or
update workflow`). CI 待接入，需 workflow scope，暂由 box 人工门禁替代：every
release is gated by the four commands in README's Gate section, run on the box
against the exact tree that is pushed.
