# Upstream quirks we reproduce on purpose

The acceptance target is what `xlsx2csv` 0.8.6 (commit `ec1883bb`) actually
prints, not what it ought to print. Each item below was observed in the
goldens before it was copied; nothing here is a guess.

| # | quirk | where it bites | evidence |
|---|---|---|---|
| 1 | `-i/--ignoreempty` skips blank rows but does **not** advance the internal `lastRowNum`, so later rows are not padded from it | rows after a skipped blank row | upstream `Sheet.handleEndElement` row branch |
| 2 | `[Content_Types].xml` with several worksheet overrides keeps only the **last** one | workbooks with merged/incremental parts | upstream `ContentTypes.handleEndElement` assignment |
| 3 | shared-string cells rewrite the literal `_x000D_\n` sequence to a plain newline | text pasted from CRLF sources | f5-quirks |
| 4 | a `General` number is printed via `%f` with trailing zeros stripped, six decimals of resolution | `1e-7` prints `0`, `0.1+0.2` prints `0.3` | f3-floats |
| 5 | a date-styled serial `< 1` is emitted as a **time**, not a date | serial `0` prints `00:00` | f2-dates |
| 6 | percentages are rounded with `ROUND_HALF_UP` on `Decimal` after `*100`; `0%` quantises to integer | `0.9995` with `0%` prints `100%`, `-0.135` prints `-14%` | f2-dates |
| 7 | `csv.writer` quotes a lone empty field (`""`) but never quotes a field for containing `\r` alone | single-column empty rows | f1/f5 |
| 8 | `-a` to stdout prefixes every sheet with `<sheetdelimiter> <index> - <name>` | multi-sheet dumps | f4-sheets `-a` |

Raised as upstream bugs? No. They are documented here so a future fix is a
deliberate decision with a golden update, not an accident.
