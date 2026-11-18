# xlsx2csv → Rust 移植：立项与验收（Chief 批准版）

状态：**批准开工（D1 起）**　批准人：Chief　日期：2026-11-18 23:0x

## 1. 对象（已钉死）

| 项 | 值 |
|---|---|
| 上游 | `dilshod/xlsx2csv`，Python，MIT |
| 锚点 commit | `ec1883bb438ee4baf05953695009b9b6e9cad966`（2026-11-10，PR #317 quoted numFmt 修复） |
| 源码 | `upstream/xlsx2csv.py`，**1386 行 / 59,972 B**，单文件，仅用 stdlib（zipfile + xml） |
| 热度 | 1784★ / 141 open issues，2026-11-16 仍有推送 |
| Rust 竞品 | `zitsen/xlsx2csv.rs` 82★，2023-11 停更 → 有竞品但已死，不构成"别做"的理由 |

## 2. 基线（我在 box 上实测，不是估算）

夹具：`fixtures/bench.xlsx`，100,000 行 × 8 列，4,627,248 B，含字符串/整数/浮点/中文/emoji/日期（numFmt `yyyy-mm-dd`）。

| 命令 | 耗时 | 峰值 RSS | 输出 |
|---|---|---|---|
| 默认 | **6.14s**（冷，含首次解压） | 23.5 MB | 100,001 行 / 6,738,002 B |
| `-d /` | 4.46s | 23.5 MB | 同上 |
| `-s 1` | 4.75s | 23.5 MB | 同上 |
| `-a`（全部工作表） | 7.54s | 23.6 MB | 写出 `S1.csv` |

**采用热基线 4.46s 作为分母**（三次取中位，去掉 0.3s Python 启动的噪声）。Scout 报的 4.09s/3.0MB 是另一份夹具，量级一致但不采用。

## 3. 验收口径（缺一不可）

1. **字节一致**：6 个夹具上 `cmp` 与上游 master 逐字节一致（goldens 由上游跑出、进仓库、带 `.meta.json` 记 commit+日期）。
2. **速度**：`fixtures/bench.xlsx` 中位耗时 **≤1.50s**（对热基线 ≥3×），取 3 次中位数。
3. **内存**：峰值 RSS **≤48MB**（不高于上游 2 倍）。
4. **门禁**：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`rust-anything gate` 全 0。
5. **不许照抄 bug**：验收只锚 master 在夹具上的实际输出；141 个 issue 里的已知缺陷不实现、不"顺手修"，遇到就记进 `KNOWN-GAPS.md`。

## 4. 范围（做 / 不做）

做：xlsx(OOXML) → CSV/TSV；单个/多个工作表；`-d` 分隔符、`-a` 全部表、`-s`/`-n` 选表、`-p` 多表分隔、`-l` 行尾、`-q` 引号、`-f`/`-t` 日期时间格式、`--floatformat`、`--skipemptycolumns`、`--ignore-percentage`。

不做：`.xls`/`.xlsb`/`.ods`、公式计算、样式输出、图片、`--hyperlinks`、stdin 输入（`-`）。

> 注意：**`-d` 是 delimiter（分隔符），不是日期格式**；日期格式是 `-f`，时间格式是 `-t`。Scout 的原始提案把 `-d` 当成日期格式写错了，以本节为准。

## 5. 夹具（6 个，D1 内冻结）

| 夹具 | 覆盖 |
|---|---|
| `f1-basic.xlsx` | 12 行 × 6 列，中文/emoji/空单元格/含逗号与双引号的字段 |
| `f2-dates.xlsx` | numFmt `yyyy-mm-dd`、`yyyy/m/d hh:mm`、百分比 `0.00%`、`-f`/`-t` 覆盖 |
| `f3-floats.xlsx` | 0.1+0.2、科学计数、19 位大整数、`-0`、`--floatformat` |
| `f4-sheets.xlsx` | 3 工作表（含一个隐藏），验 `-a` 的 `S1.csv` 命名与 `-s 2` |
| `f5-quirks.xlsx` | 合并单元格、富文本 `rPr`、`xml:space="preserve"`、`&amp;` 转义、超长单格 |
| `f6-big` | 即 `bench.xlsx`，只做抽样比对 + 性能 |

## 6. 分工与时间

- **Rustsmith**（本机，只写代码）：`src/port.rs` 按上游算法逐段移植；`src/bin/main.rs` 接 CLI；`tools/parity.sh`；`KNOWN-GAPS.md`。本机不能编译，写完 push。
- **Chief**（box，唯一构建环境）：clone → `rust-anything analyze/bridge/record` → 编译 → 跑 goldens 与 bench → 出 diff 报告。
- **Prober**：拿 diff 报告与 KNOWN-GAPS 做独立复核，出 issue。

| 日 | 目标 |
|---|---|
| D1（11-19） | 上游 vendored、6 夹具与 goldens 生成、`rust-anything analyze --whole-app` + `bridge` + `record --count 2000`、`port.rs` 第一版（能解析出 sheet 与 sharedStrings） |
| D2（11-20） | port 收敛：f1–f5 逐字节对齐；f6 抽检 |
| D3（11-21 上午） | bench（≥3×）→ gate → 复算报告 → 是否转 public 仓由我定 |

## 7. 风险与对策

1. 浮点与日期格式最易字节漂移 → D1 先冻结格式表与 goldens，再写格式化代码。
2. sharedStrings 全量入内存（上游如此）→ 先与上游同构，别急着流式；流式必须证明输出不变。
3. 合并单元格/富文本按实测输出为准，不按 OOXML 规范想象。
4. box 只有 1 CPU 且是唯一构建环境 → 大夹具不进 git，用 `fixtures/gen_xlsx.py` 现场生成。
5. 2.5 天硬边界：D3 上午没到 ≥3× 就停下来复盘，不许拖成第三周。
