# Benchmarks

`run.rb` compares plain roo, roo with roo-calamine, and
[calx](https://github.com/7a6163/calx) on four generated workbooks.

## Run it

```sh
cd bench && bundle install
bundle exec ruby run.rb                  # every library, 3–10 runs per scenario, stopping after 10 s
bundle exec ruby run.rb 5 roo-calamine   # 5 runs, and only the libraries named
bundle exec ruby scenario.rb roo-calamine yaml fixtures/mixed-100k.xlsx  # one run of one scenario
```

`run.rb` writes the fixtures on its first run (`generate.rb`). Append `digest`
to a `scenario.rb` call to hash every value it reads.

## Method

- **Each run is a fresh process.** The timer starts after `require` and
  includes opening the workbook. Memory is the process's peak RSS.
- **The table shows medians.** Without a run count, each scenario runs 3–10
  times and stops once it has run for 10 s.
- **Before timing**, one run of roo and one of roo-calamine hash every value
  they read, and the run stops if the two hashes differ.
- **Values only:** plain roo runs `each_row_streaming(pad_cells: true)` and
  reads each cell's value.
- **calx** is a reference, not a drop-in replacement:
  - its values follow its own rules: a whole number is an Integer whatever
    the cell's format, a date with a time is a local `Time`, and there are no
    roo cells or `Roo::Link`s;
  - it loads the whole sheet before it yields the first row;
  - it has no `to_yaml`, `to_xml`, or `to_matrix`.

## Results

Apple M5 Pro, Ruby 3.4.12, roo 4.0.0, calx 0.1.0.
- **roo-calamine:** the median of 5 runs on 2660f9a.
- **roo and calx:** from the same machine on 65bd561. Neither changes with
  roo-calamine.
- **roo's `to_yaml`, `to_xml`, and `to_matrix`:** one run each on the mixed
  file, from #58.

### 100k rows × 20 mixed columns

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 |
|---|---:|---:|---:|---:|
| `each_row_streaming` | 13.49 s · 119 MB | **0.59 s · 51 MB** | 23× | 0.52 s · 178 MB |
| `parse(headers: true)` | 19.61 s · 3300 MB | **0.75 s · 282 MB** | 26× | 0.71 s · 290 MB |
| `each` | 18.81 s · 3282 MB | **0.62 s · 174 MB** | 30× | 0.45 s · 178 MB |
| 10,000 random `cell(row, col)` | 17.25 s · 3272 MB | **0.48 s · 104 MB** | 36× | 0.51 s · 250 MB |
| `to_csv` | 21.72 s · 3369 MB | **0.77 s · 118 MB** | 28× | 0.66 s · 178 MB |
| values only: `each_row_values` | 13.43 s · 118 MB | **0.44 s · 47 MB** | 31× | 0.52 s · 178 MB |
| `to_yaml` | 24.97 s | **1.28 s · 322 MB** | 20× | — |
| `to_xml` | 53.29 s | **1.04 s · 711 MB** | 51× | — |
| `to_matrix` | 20.70 s | **0.65 s · 216 MB** | 32× | — |

### 100k rows × 10 shared strings

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 |
|---|---:|---:|---:|---:|
| `each_row_streaming` | 4.92 s · 121 MB | **0.26 s · 52 MB** | 19× | 0.24 s · 114 MB |
| `parse(headers: true)` | 6.02 s · 1630 MB | **0.34 s · 117 MB** | 18× | 0.33 s · 208 MB |
| `each` | 5.66 s · 1631 MB | **0.27 s · 74 MB** | 21× | 0.20 s · 113 MB |
| 10,000 random `cell(row, col)` | 4.93 s · 1602 MB | **0.22 s · 71 MB** | 22× | 0.22 s · 167 MB |
| `to_csv` | 7.17 s · 1619 MB | **0.25 s · 88 MB** | 29× | 0.22 s · 114 MB |
| values only: `each_row_values` | 5.12 s · 120 MB | **0.19 s · 50 MB** | 27× | 0.24 s · 113 MB |
| `to_yaml` | — | **0.46 s · 143 MB** | — | — |
| `to_xml` | — | **0.35 s · 308 MB** | — | — |
| `to_matrix` | — | **0.28 s · 97 MB** | — | — |

### 100k rows × 10 unique strings

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 |
|---|---:|---:|---:|---:|
| `each_row_streaming` | 10.27 s · 933 MB | **0.48 s · 250 MB** | 21× | 0.38 s · 152 MB |
| `parse(headers: true)` | 9.07 s · 2327 MB | **0.58 s · 211 MB** | 16× | 0.47 s · 206 MB |
| `each` | 8.80 s · 2262 MB | **0.48 s · 170 MB** | 18× | 0.34 s · 152 MB |
| 10,000 random `cell(row, col)` | 8.49 s · 2213 MB | **0.42 s · 157 MB** | 20× | 0.36 s · 206 MB |
| `to_csv` | 10.40 s · 2366 MB | **0.48 s · 175 MB** | 22× | 0.37 s · 153 MB |
| values only: `each_row_values` | 10.32 s · 925 MB | **0.38 s · 160 MB** | 27× | 0.37 s · 152 MB |
| `to_yaml` | — | **0.69 s · 238 MB** | — | — |
| `to_xml` | — | **0.59 s · 417 MB** | — | — |
| `to_matrix` | — | **0.48 s · 183 MB** | — | — |

### 100 rows × 20 mixed columns

| Scenario | roo 4.0.0 | roo + roo-calamine | Faster | calx 0.1.0 |
|---|---:|---:|---:|---:|
| `each_row_streaming` | 15.9 ms · 45 MB | **2.1 ms · 41 MB** | 7.6× | 0.7 ms · 27 MB |
| `parse(headers: true)` | 12.8 ms · 45 MB | **2.0 ms · 41 MB** | 6.4× | 0.7 ms · 27 MB |
| `each` | 12.4 ms · 45 MB | **1.9 ms · 40 MB** | 6.5× | 0.6 ms · 27 MB |
| 10,000 random `cell(row, col)` | 19.4 ms · 46 MB | **7.5 ms · 41 MB** | 2.6× | 1.5 ms · 27 MB |
| `to_csv` | 15.3 ms · 46 MB | **2.1 ms · 41 MB** | 7.3× | 0.8 ms · 27 MB |
| values only: `each_row_values` | 16.2 ms · 45 MB | **1.9 ms · 41 MB** | 8.5× | 0.7 ms · 27 MB |
| `to_yaml` | — | **2.5 ms · 41 MB** | — | — |
| `to_xml` | — | **2.3 ms · 41 MB** | — | — |
| `to_matrix` | — | **3.5 ms · 41 MB** | — | — |
