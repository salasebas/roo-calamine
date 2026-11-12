# Benchmarks

`run.rb` compares plain roo, roo with roo-calamine,
[calx](https://github.com/7a6163/calx), and [xsv](https://github.com/martijn/xsv)
on generated workbooks.

## Run it

```sh
cd bench && bundle install
bundle exec ruby run.rb                  # the tables below, 3–10 runs per scenario, stopping after 10 s
bundle exec ruby run.rb 5 roo-calamine   # 5 runs, and only the libraries named
bundle exec ruby run.rb --scenario row,column --fixture mixed-100k roo-calamine  # only these rows
bundle exec ruby run.rb --group single-cell                  # only this group
bundle exec ruby run.rb --all                                # every scenario on every file, as in RESULTS.md
bundle exec ruby run.rb --profile each-headers mixed-100k    # vernier profile and allocations
bundle exec ruby scenario.rb roo-calamine yaml fixtures/mixed-100k.xlsx  # one run of one scenario
```

`run.rb` writes the fixtures it needs (`generate.rb`). Append `digest` to a
`scenario.rb` call to hash every value it reads. `run.rb --help` lists the
groups: `default`, `open`, `rows`, `single-cell`, `iteration`, `builders`,
`writes`, `metadata`, and `options`. For the Rust side, record a `scenario.rb`
call with `samply record`.

## Method

- **Each run is a fresh process.** The timer starts after `require` and
  includes opening the workbook. Memory is the process's peak RSS.
- **The table shows medians.** Without a run count, each scenario runs 3–10
  times and stops once it has run for 10 s.
- **Before timing**, one run of roo and one of roo-calamine hash every value
  they read, and the run stops if the two hashes differ.
- **Values only:** plain roo runs `each_row_streaming(pad_cells: true)` and
  reads each cell's value.
- **Flags:** a row is flagged when roo-calamine is less than 5× faster than
  roo, or more than 1.2× slower than calx.
  [RESULTS.md](RESULTS.md#flagged) says why for each one.
- **calx and xsv** are references, not drop-in replacements:
  - their values follow their own rules: calx makes a whole number an
    Integer whatever the cell's format and a date with a time a local
    `Time`, and neither has roo cells or `Roo::Link`s;
  - calx loads the whole sheet before it yields the first row;
  - neither has `to_yaml`, `to_xml`, or `to_matrix`, and random `cell` reads
    load every row first.

## Results

Apple M5 Pro, Ruby 3.4.12, roo 4.0.0, calx 0.1.0, xsv 1.4.1.
Iteration timings use five fresh processes per scenario. `each` on
`mixed-100k.xlsx` uses 10 runs on be4ab9b for roo-calamine and calx. Other
rows keep their earlier measurements.

[RESULTS.md](RESULTS.md) has every other scenario and file.

### 100k rows × 20 mixed columns

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 | xsv 1.4.1 |
|---|---:|---:|---:|---:|---:|
| `each_row_streaming` | 13.49 s · 119 MB | **0.55 s · 51 MB** | 24.6× | 0.52 s · 179 MB | 9.66 s · 51 MB |
| `parse(headers: true)` | 19.61 s · 3300 MB | **0.75 s · 282 MB** | 26× | 0.71 s · 290 MB | 11.18 s · 203 MB |
| `each` | 18.81 s · 3282 MB | **0.32 s · 174 MB** | 59× | 0.47 s · 178 MB | 9.77 s · 51 MB |
| 10,000 random `cell(row, col)` | 17.25 s · 3272 MB | **0.48 s · 104 MB** | 36× | 0.51 s · 250 MB | 10.89 s · 172 MB |
| `to_csv` | 21.72 s · 3369 MB | **0.77 s · 118 MB** | 28× | 0.66 s · 178 MB | 9.85 s · 50 MB |
| values only: `each_row_values` | 13.43 s · 118 MB | **0.43 s · 48 MB** | 31.3× | 0.51 s · 179 MB | 9.65 s · 51 MB |
| `to_yaml` | 24.26 s · 3430 MB | **1.28 s · 322 MB** | 19× | — | — |
| `to_xml` | 50.81 s · 3991 MB | **1.04 s · 711 MB** | 49× | — | — |
| `to_matrix` | 19.97 s · 3287 MB | **0.65 s · 216 MB** | 31× | — | — |

### 100k rows × 10 shared strings

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 | xsv 1.4.1 |
|---|---:|---:|---:|---:|---:|
| `each_row_streaming` | 4.92 s · 121 MB | **0.24 s · 53 MB** | 20.4× | 0.24 s · 114 MB | 4.19 s · 48 MB |
| `parse(headers: true)` | 6.02 s · 1630 MB | **0.34 s · 117 MB** | 18× | 0.33 s · 208 MB | 4.72 s · 108 MB |
| `each` | 5.66 s · 1631 MB | **0.24 s · 79 MB** | 23.5× | 0.20 s · 114 MB | 4.26 s · 54 MB |
| 10,000 random `cell(row, col)` | 4.93 s · 1602 MB | **0.22 s · 71 MB** | 22× | 0.22 s · 167 MB | 4.68 s · 85 MB |
| `to_csv` | 7.17 s · 1619 MB | **0.25 s · 88 MB** | 29× | 0.22 s · 114 MB | 4.28 s · 51 MB |
| values only: `each_row_values` | 5.12 s · 120 MB | **0.19 s · 50 MB** | 27× | 0.24 s · 113 MB | 4.33 s · 49 MB |
| `to_yaml` | 8.24 s · 1696 MB | **0.46 s · 143 MB** | 18× | — | — |
| `to_xml` | 15.07 s · 2054 MB | **0.35 s · 308 MB** | 43× | — | — |
| `to_matrix` | 5.77 s · 1634 MB | **0.28 s · 97 MB** | 21× | — | — |

### 100k rows × 10 unique strings

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 | xsv 1.4.1 |
|---|---:|---:|---:|---:|---:|
| `each_row_streaming` | 10.27 s · 933 MB | **0.47 s · 254 MB** | 22.0× | 0.37 s · 152 MB | 14.88 s · 254 MB |
| `parse(headers: true)` | 9.07 s · 2327 MB | **0.58 s · 211 MB** | 16× | 0.47 s · 206 MB | 15.18 s · 347 MB |
| `each` | 8.80 s · 2262 MB | **0.44 s · 174 MB** | 20.0× | 0.34 s · 152 MB | 11.21 s · 248 MB |
| 10,000 random `cell(row, col)` | 8.49 s · 2213 MB | **0.42 s · 157 MB** | 20× | 0.36 s · 206 MB | 14.89 s · 295 MB |
| `to_csv` | 10.40 s · 2366 MB | **0.48 s · 175 MB** | 22× | 0.37 s · 153 MB | 10.09 s · 286 MB |
| values only: `each_row_values` | 10.32 s · 925 MB | **0.38 s · 160 MB** | 27× | 0.37 s · 152 MB | 10.06 s · 265 MB |
| `to_yaml` | 11.43 s · 2424 MB | **0.69 s · 238 MB** | 17× | — | — |
| `to_xml` | 19.37 s · 2511 MB | **0.59 s · 417 MB** | 33× | — | — |
| `to_matrix` | 9.06 s · 2312 MB | **0.48 s · 183 MB** | 19× | — | — |

### 100 rows × 20 mixed columns

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 | xsv 1.4.1 |
|---|---:|---:|---:|---:|---:|
| `each_row_streaming` | 15.9 ms · 45 MB | **2.0 ms · 42 MB** | 8.0× | 0.7 ms · 27 MB | 11.5 ms · 42 MB |
| `parse(headers: true)` | 12.8 ms · 45 MB | **2.0 ms · 41 MB** | 6.4× | 0.7 ms · 27 MB | 11.5 ms · 42 MB |
| `each` | 12.4 ms · 45 MB | **1.9 ms · 42 MB** | 6.7× | 0.6 ms · 27 MB | 11.4 ms · 42 MB |
| 10,000 random `cell(row, col)` | 19.4 ms · 46 MB | **7.5 ms · 41 MB** | 2.6× | 1.5 ms · 27 MB | 12.5 ms · 43 MB |
| `to_csv` | 15.3 ms · 46 MB | **2.1 ms · 41 MB** | 7.3× | 0.8 ms · 27 MB | 12.9 ms · 42 MB |
| values only: `each_row_values` | 16.2 ms · 45 MB | **1.9 ms · 41 MB** | 8.5× | 0.7 ms · 27 MB | 13.9 ms · 43 MB |
| `to_yaml` | 16.5 ms · 46 MB | **2.5 ms · 41 MB** | 6.6× | — | — |
| `to_xml` | 23.5 ms · 47 MB | **2.3 ms · 41 MB** | 10× | — | — |
| `to_matrix` | 15.0 ms · 46 MB | **3.5 ms · 41 MB** | 4.3× | — | — |
