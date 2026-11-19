#!/usr/bin/env python3
"""Generate the seven xlsx parity fixtures for xlsx2csv-rs.

Covers: f1 basic/unicode, f2 dates+percent, f3 floats, f4 multi-sheet
(incl. hidden), f5 quirks (merged / rich text / entities / long cell /
formula with cached value), f6 big (50000 rows x 8 cols, bench input).

Run: python3 gen_fixtures.py [OUTDIR]   (default: next to this file)
"""
import datetime as dt
import os
import sys
import re
import zipfile

from openpyxl import Workbook
from openpyxl.cell import WriteOnlyCell

HERE = os.path.dirname(os.path.abspath(__file__))
# Where the .xlsx files go. Overridden by argv[1] (see main), so the fixtures
# can be generated outside the repository (issue #1: the output directory used
# to be ignored, which made "run it into /tmp" silently write into fixtures/).
OUT = HERE
# Every file save() writes, so main() can stamp them all at the end.
PATHS = []
# A fixture must be byte-identical on every run (issue #1): openpyxl would
# otherwise stamp docProps/core.xml and every zip entry with the current time.
FIXED_TIME = (1980, 1, 1, 0, 0, 0)
FIXED_STAMP = b"2024-01-01T00:00:00Z"


def save(wb, name):
    path = os.path.join(OUT, name)
    wb.save(path)
    PATHS.append(path)
    print("%-18s %8d bytes" % (name, os.path.getsize(path)))
    return path


def normalize(path):
    """Rewrite a workbook with fixed timestamps so its bytes are reproducible."""
    tmp = path + ".tmp"
    with zipfile.ZipFile(path) as zin, zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "docProps/core.xml":
                data = re.sub(
                    rb"(<dcterms:(?:created|modified)[^>]*>)[^<]*(<)",
                    lambda m: m.group(1) + FIXED_STAMP + m.group(2),
                    data,
                )
            zi = zipfile.ZipInfo(item.filename, date_time=FIXED_TIME)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zi.external_attr = item.external_attr
            zout.writestr(zi, data)
    os.replace(tmp, path)
    return path


def patch_zip(path, member, fn):
    """Rewrite one zip member through fn(text)->text; returns 1 if it changed."""
    tmp = path + ".tmp"
    hits = 0
    with zipfile.ZipFile(path) as zin, zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == member:
                before = data.decode("utf-8")
                after = fn(before)
                if after != before:
                    hits = 1
                data = after.encode("utf-8")
            zout.writestr(item, data)
    os.replace(tmp, path)
    return hits


def f1_basic():
    wb = Workbook()
    ws = wb.active
    ws.title = "basic"
    rows = [
        ["id", "name", "note", "comma,field", 'quote"field', "empty"],
        [1, "中文", "emoji \U0001f600\U0001f680", "a,b", 'say "hi"', None],
        [2, "line\nbreak", "tab\there", "  leading", "trailing  ", ""],
        [3, "0123", "=notformula", "x;y", "semi;colon", None],
        [4, "省略号\u2026", "\u00dcn\u00efcode", "", "  ", None],
        [5, "https://example.com/a?b=1&c=2", "amp", 'a"b,c', "z", None],
        [6, 1234567, -42, 0, 3.5, None],
        [7, "", "", "", "", None],
        [8, "  ", "空格", "\u00a0nbsp", "end", None],
        [9, "末行", "tail", "t", "u", None],
        [10, "多语言", "\u0627\u0644\u0639\u0631\u0628\u064a\u0629", "\u0440\u0443\u0441\u0441\u043a\u0438\u0439", "\u65e5\u672c\u8a9e", None],
        [11, "emoji \U0001f468\u200d\U0001f469\u200d\U0001f467\u200d\U0001f466", "组合", "a\tb", "c\nd", None],
    ]
    for row in rows:
        ws.append(row)
    return save(wb, "f1-basic.xlsx")


def f2_dates():
    wb = Workbook()
    ws = wb.active
    ws.title = "dates"
    ws.append(["date", "datetime", "time", "pct2", "pct0", "textdate"])
    rows = [
        [dt.date(2026, 11, 19), dt.datetime(2026, 11, 19, 13, 45, 30), dt.time(7, 5, 9),
         0.135, 0.00125, "2026-11-19"],
        [dt.date(1899, 12, 30), dt.datetime(1904, 1, 1), dt.time(23, 59, 59),
         1.0, 0.9995, "x"],
        [dt.date(2026, 1, 1), dt.datetime(2026, 1, 1, 0, 0, 0), dt.time(0, 0, 0),
         0.0, -0.135, "y"],
        [dt.date(2024, 2, 29), dt.datetime(2000, 2, 29, 23, 0, 0), dt.time(12, 0, 0),
         0.5, 0.5, "2024-02-29"],
    ]
    for row in rows:
        ws.append(row)
    fmts = ["yyyy-mm-dd", "yyyy/m/d hh:mm", "hh:mm:ss", "0.00%", "0%", None]
    for r in ws.iter_rows(min_row=2, max_row=5):
        for cell, fmt in zip(r, fmts):
            if fmt:
                cell.number_format = fmt
    return save(wb, "f2-dates.xlsx")


def f3_floats():
    wb = Workbook()
    ws = wb.active
    ws.title = "floats"
    ws.append(["plain", "fmt0.00", "sci"])
    plain = [0.1 + 0.2, 1e-07, 1234567890123456789, -0.0, 2.5, 100.0, 1e20, -2.75]
    fmt2 = [0.30000000000000004, 1.5, -0.5, 0.005, 2.675, 1e-05, 1.0, 7.0]
    sci = [1e-07, 1.0e20, 2.5, -3.125, 6.02e23, 1.5e-300, 9.999e15, 0.0]
    for a, b, c in zip(plain, fmt2, sci):
        ws.append([a, b, c])
    for r in ws.iter_rows(min_row=2, max_row=9):
        r[1].number_format = "0.00"
        r[2].number_format = "0.00E+00"
    return save(wb, "f3-floats.xlsx")


def f4_sheets():
    wb = Workbook()
    s1 = wb.active
    s1.title = "S1"
    s1.append(["a", "b"])
    s1.append([1, 2])
    s2 = wb.create_sheet("S2 名称")
    s2.append(["x"])
    s2.append(["y"])
    s2.append([])
    s3 = wb.create_sheet("hidden-sheet")
    s3.append(["h", "i"])
    s3["B1"] = "隐藏"
    s3.sheet_state = "hidden"
    return save(wb, "f4-sheets.xlsx")


def f5_quirks():
    wb = Workbook()
    ws = wb.active
    ws.title = "quirks"
    ws["A1"] = "merged-top"
    ws.merge_cells("A1:B1")
    ws["A2"] = "left"
    ws["B2"] = "right"
    ws["A3"] = "&<>\"'"
    ws["A4"] = "x" * 2000
    ws["A5"] = "  preserved  "
    ws["A6"] = "tab\tinside"
    ws["A7"] = "line\ninside"
    ws["A8"] = "=1+2"
    ws["B8"] = "=SUM(1,2)"
    try:
        from openpyxl.cell.rich_text import CellRichText, TextBlock
        from openpyxl.cell.text import InlineFont
        ws["A9"] = CellRichText([TextBlock(InlineFont(b=True), "粗体"), "普通"])
    except Exception as exc:  # pragma: no cover - depends on openpyxl build
        print("rich text skipped:", exc)
    path = save(wb, "f5-quirks.xlsx")

    def add_cached(xml):
        # openpyxl writes `<v></v>` (older builds: `<v/>`) for a formula cell.
        xml = re.sub(r"<v\s*/>", "<v></v>", xml)
        for ref, value in (("A8", "3"), ("B8", "3")):
            pat = r'(<c r="%s"[^>]*><f>[^<]*</f>)<v></v>' % ref
            xml = re.sub(pat, lambda m, v=value: m.group(1) + "<v>" + v + "</v>", xml, count=1)
        return xml

    hits = patch_zip(path, "xl/worksheets/sheet1.xml", add_cached)
    print("f5 cached-value patch applied:", hits)
    return path


def f6_big(rows=50000):
    wb = Workbook(write_only=True)
    ws = wb.create_sheet("big")

    def c(value, fmt=None):
        cell = WriteOnlyCell(ws, value=value)
        if fmt:
            cell.number_format = fmt
        return cell

    ws.append([c(v) for v in ("id", "名称", "金额", "比例", "日期", "flag", "备注", "col8")])
    base = dt.date(2026, 1, 1)
    for i in range(rows):
        ws.append([
            c(i),
            c("行%d 中文\U0001f600" % i),
            c(((i * 37) % 10000) / 100.0, "0.00"),
            c((i % 100) / 100.0, "0.00%"),
            c(base + dt.timedelta(days=i % 365), "yyyy-mm-dd"),
            c(i % 2 == 0),
            c("" if i % 7 == 0 else "note-%d" % i),
            c((i * 13) % 97),
        ])
    return save(wb, "f6-big.xlsx")



def f7_hyperlinks():
    """Hyperlinked cells: plain text, unicode, empty, mailto, date, percent,
    number, and a range ref patched in afterwards (A6:A7)."""
    wb = Workbook()
    ws = wb.active
    ws.title = "links"
    ws["A1"] = "site"
    ws["A1"].hyperlink = "https://example.com/a?b=1"
    ws["B1"] = "plain"
    ws["A2"] = "\u4e2d\u6587\u94fe\u63a5"
    ws["A2"].hyperlink = "https://example.org/\u8def\u5f84"
    ws["A3"].hyperlink = "https://example.net/empty"
    ws["A4"] = "mail"
    ws["A4"].hyperlink = "mailto:x@example.com"
    ws["A5"] = dt.date(2023, 3, 15)
    ws["A5"].number_format = "yyyy-mm-dd"
    ws["A5"].hyperlink = "https://example.com/day"
    ws["A6"] = 0.5
    ws["A6"].number_format = "0.00%"
    ws["A6"].hyperlink = "https://example.com/pct"
    ws["A7"] = 42
    ws["A8"] = "tab\tinside"
    ws["A8"].hyperlink = "https://example.com/tab"
    path = save(wb, "f7-hyperlinks.xlsx")

    def widen(xml):
        return xml.replace('ref="A6"', 'ref="A6:A7"', 1)

    hits = patch_zip(path, "xl/worksheets/sheet1.xml", widen)
    print("f7 range patch applied:", hits)
    return path



def f8_merged():
    """`-m` fixture: a merge whose follower cells really exist in the sheet
    XML (openpyxl omits them, so they are patched back in), both horizontal
    and vertical."""
    wb = Workbook()
    ws = wb.active
    ws.title = "merged"
    ws["A1"] = "wide"
    ws.merge_cells("A1:C1")
    ws["A2"] = 7
    ws.merge_cells("A2:B2")
    ws["D1"] = "tail"
    path = save(wb, "f8-merged.xlsx")

    def add_followers(xml):
        xml = xml.replace('<c r="D1"', '<c r="B1"/><c r="C1"/><c r="D1"', 1)
        return xml.replace(
            '<c r="A2" t="n"><v>7</v></c>', '<c r="A2" t="n"><v>7</v></c><c r="B2"/>', 1
        )

    hits = patch_zip(path, "xl/worksheets/sheet1.xml", add_followers)
    print("f8 follower patch applied:", hits)
    return path


def f9_shared_strings():
    """`-e` fixture: real sharedStrings table. openpyxl 3.1.5 emits inline
    strings, so the cells are rewritten to `t="s"` and a sharedStrings part is
    added -- otherwise `-e` has nothing to escape."""
    wb = Workbook()
    ws = wb.active
    ws.title = "sst"
    ws["A1"] = "tab\tinside"
    ws["A2"] = "line\ninside"
    ws["A3"] = "carriage\rreturn"
    ws["A4"] = "plain"
    ws["A5"] = "中文\temoji \U0001f600"
    ws["B1"] = 1
    path = save(wb, "f9-shared.xlsx")

    cell_re = re.compile(
        r'<c r="([A-Z]+\d+)"([^>]*?)t="inlineStr"><is><t[^>]*>(.*?)</t></is></c>'
    )
    ctype_override = (
        '<Override PartName="/xl/sharedStrings.xml" ContentType='
        '"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>'
    )
    rel = (
        '<Relationship Id="rIdSst" Type="http://schemas.openxmlformats.org/'
        'officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>'
    )

    with zipfile.ZipFile(path) as zin:
        items = [(i, zin.read(i.filename)) for i in zin.infolist()]
    strings = []
    out = []
    for item, data in items:
        if item.filename == "xl/worksheets/sheet1.xml":
            text = data.decode("utf-8")

            def repl(m):
                strings.append(m.group(3))
                return '<c r="%s"%s t="s"><v>%d</v></c>' % (
                    m.group(1), m.group(2).rstrip(" "), len(strings) - 1
                )

            data = cell_re.sub(repl, text).encode("utf-8")
        elif item.filename == "[Content_Types].xml":
            data = data.decode("utf-8").replace("</Types>", ctype_override + "</Types>", 1).encode("utf-8")
        elif item.filename == "xl/_rels/workbook.xml.rels":
            data = data.decode("utf-8").replace("</Relationships>", rel + "</Relationships>", 1).encode("utf-8")
        out.append((item, data))

    sst = '<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="%d" uniqueCount="%d">' % (len(strings), len(strings))
    for t in strings:
        sst += '<si><t xml:space="preserve">%s</t></si>' % t
    sst += "</sst>"

    tmp = path + ".tmp"
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
        for item, data in out:
            zout.writestr(item, data)
        zout.writestr("xl/sharedStrings.xml", sst.encode("utf-8"))
    os.replace(tmp, path)
    print("f9 shared strings:", len(strings))
    return path


def main(argv):
    global OUT
    if len(argv) > 1:
        OUT = os.path.abspath(argv[1])
    os.makedirs(OUT, exist_ok=True)
    f1_basic()
    f2_dates()
    f3_floats()
    f4_sheets()
    f5_quirks()
    f6_big()
    f7_hyperlinks()
    f8_merged()
    f9_shared_strings()
    for path in PATHS:
        normalize(path)


if __name__ == "__main__":
    main(sys.argv)
