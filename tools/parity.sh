#!/bin/sh
# Byte-for-byte parity against the tracked goldens.
#
#   tools/parity.sh [candidate-binary] [fixtures-dir]
#
# Defaults: target/release/xlsx2csv-rs and fixtures/.
# Exit 0 = every case matched, 1 = a mismatch, 2 = usage/setup problem.
set -u

BIN=${1:-target/release/xlsx2csv-rs}
FIX=${2:-fixtures}

# `nargs='+'` options swallow a trailing path, so these cases put the input first,
# the way argparse expects: `xlsx2csv <file> -I <pattern>`.
check_file_first() {
    fixture=$1
    golden=$2
    shift 2
    xlsx="$FIX/$fixture.xlsx"
    want="$FIX/$golden"
    if [ ! -f "$xlsx" ] || [ ! -f "$want" ]; then
        echo "SKIP $fixture $* (missing fixture or golden)"
        skip=$((skip + 1))
        return
    fi
    if [ -n "${UPSTREAM:-}" ]; then
        $UPSTREAM "$xlsx" "$@" >"$tmp/want" 2>/dev/null || true
        if ! cmp -s "$tmp/want" "$want"; then
            echo "FAIL $fixture $* -> golden differs from a fresh upstream run"
            fail=$((fail + 1))
            return
        fi
    fi
    "$BIN" "$xlsx" "$@" >"$tmp/got" 2>"$tmp/err"
    rc=$?
    if [ "$rc" -eq 0 ] && cmp -s "$tmp/got" "$want"; then
        echo "PASS $fixture $* -> $golden"
        pass=$((pass + 1))
    else
        off=$(cmp "$tmp/got" "$want" 2>&1 | head -1)
        echo "FAIL $fixture $* -> $golden (rc=$rc) $off"
        fail=$((fail + 1))
    fi
}

[ -x "$BIN" ] || { echo "parity: candidate not executable: $BIN" >&2; exit 2; }
[ -d "$FIX" ] || { echo "parity: fixtures dir missing: $FIX" >&2; exit 2; }

tmp=$(mktemp -d) || exit 2
trap 'rm -rf "$tmp"' EXIT INT TERM

pass=0
fail=0
skip=0

check() {
    fixture=$1
    golden=$2
    shift 2
    xlsx="$FIX/$fixture.xlsx"
    want="$FIX/$golden"
    if [ ! -f "$xlsx" ]; then
        echo "SKIP $fixture $* (no $xlsx)"
        skip=$((skip + 1))
        return
    fi
    if [ ! -f "$want" ]; then
        echo "SKIP $fixture $* (no $want)"
        skip=$((skip + 1))
        return
    fi
    "$BIN" "$@" "$xlsx" >"$tmp/got" 2>"$tmp/err"
    rc=$?
    if [ "$rc" -eq 0 ] && cmp -s "$tmp/got" "$want"; then
        echo "PASS $fixture $* -> $golden"
        pass=$((pass + 1))
    else
        off=$(cmp "$tmp/got" "$want" 2>&1 | head -1)
        echo "FAIL $fixture $* -> $golden (rc=$rc) $off"
        fail=$((fail + 1))
    fi
}

check f1-basic f1-basic.golden.csv
check f1-basic f1-basic.d-semi.golden.csv -d ';'
check f2-dates f2-dates.golden.csv
check f3-floats f3-floats.golden.csv
check f4-sheets f4-sheets.golden.csv
check f4-sheets f4-sheets.a.golden.csv -a
check f4-sheets f4-sheets.s2.golden.csv -s 2
check f4-sheets f4-sheets.n.golden.csv -n 'S2 名称'
check f5-quirks f5-quirks.golden.csv
check f6-big f6-big.golden.csv
check f7-hyperlinks f7-hyperlinks.golden.csv --hyperlinks
check f7-hyperlinks f7-hyperlinks.plain.golden.csv
check f8-merged f8-merged.golden.csv -m
check f8-merged f8-merged.plain.golden.csv
check f5-quirks f5-quirks.nlb.golden.csv --no-line-breaks
check f1-basic f1-basic.nlb.golden.csv --no-line-breaks
check_file_first f4-sheets f4-sheets.i-s2.golden.csv -a -I '^S2'
check_file_first f4-sheets f4-sheets.i-two.golden.csv -a -I S1 S2
check_file_first f4-sheets f4-sheets.e-s.golden.csv -a -E '^S'
check f9-shared f9-shared.golden.csv
check f9-shared f9-shared.e.golden.csv -e

# Values the differential fuzz turned into bugs: negative fractions in time
# formats, `--floatformat` precision, and the QUOTE_NONNUMERIC rule.
check_file_first f10-edges f10-edges.golden.csv
check_file_first f10-edges f10-edges.qnonnumeric.golden.csv -q nonnumeric
check_file_first f10-edges f10-edges.ff3.golden.csv --floatformat %.3f
# Python's `%` operator, not printf: %e/%g keep two exponent digits and %g
# strips mantissa zeros only.
check_file_first f3-floats f3-floats.e.golden.csv --floatformat '%e'
check_file_first f3-floats f3-floats.g.golden.csv --floatformat '%g'
check_file_first f3-floats f3-floats.f0.golden.csv --floatformat '%.0f'

# `-c/--outputencoding` only affects file output (upstream opens the file with
# `encoding=` and leaves sys.stdout alone), so these cases compare files.
check_enc() {
    fixture=$1
    golden=$2
    enc=$3
    xlsx="$FIX/$fixture.xlsx"
    want="$FIX/$golden"
    if [ ! -f "$xlsx" ] || [ ! -f "$want" ]; then
        echo "SKIP $fixture -c $enc (missing fixture or golden)"
        skip=$((skip + 1))
        return
    fi
    mkdir -p "$tmp"
    if "$BIN" -c "$enc" "$xlsx" "$tmp/enc.out" >/dev/null 2>"$tmp/enc.err"; then :; fi
    if cmp -s "$tmp/enc.out" "$want"; then
        echo "PASS $fixture -c $enc"
        pass=$((pass + 1))
    else
        echo "FAIL $fixture -c $enc -> $golden"
        fail=$((fail + 1))
    fi
}

check_enc f3-floats f3-floats.utf16le.golden.csv utf-16-le
check_enc f3-floats f3-floats.cp1252.golden.csv cp1252
check_enc f3-floats f3-floats.utf16.golden.csv utf-16

echo "parity: $pass passed, $fail failed, $skip skipped"
[ "$fail" -eq 0 ] || exit 1
exit 0
