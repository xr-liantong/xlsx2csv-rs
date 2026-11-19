# Known gaps and divergences (v0.1.0)

Acceptance for v0.1.0: byte-identical output to upstream 0.8.6 on the ten
golden cases in `fixtures/`. Everything below is outside that net, either
because it is unimplemented or because no fixture covers it yet.

## Not implemented (exit 1 with `not implemented yet`)

| option | upstream behaviour |
|---|---|
| `-m`, `--merge-cells` | emit merged ranges as extra rows |
| `--hyperlinks` | `-m` plus hyperlink attributes from sheet rels |
| `--no-line-breaks` | strip newlines inside cell text |
| `-E`, `-I` | exclude / include sheet name patterns (nargs) |

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
