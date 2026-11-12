# Benchmark results

## Cleaning strings, issue 65

Apple M5 Pro, Ruby 3.4.12, roo 4.0.0, `mixed-100k.xlsx` with 100k rows and
20 mixed columns. Median of five fresh processes per scenario and version,
alternating the original code at 3df99e4 and the issue 65 change at 3655290,
before rebasing onto #69 and #72.
The timer includes opening the workbook. Memory is peak RSS in MiB.

| Scenario | Before | After | Time saved | Peak RSS before → after |
|---|---:|---:|---:|---:|
| `each(headers: true)` | 0.781 s | 0.798 s | -2% | 195 → 183 MiB |
| `each(clean: true, headers: true)` | 3.244 s | 1.227 s | 62% | 725 → 295 MiB |
| One `cell`, then `each(clean: true, headers: true)` | 3.758 s | 1.246 s | 67% | 706 → 295 MiB |

The cleaned read takes 1.54× the uncleaned read, down from 4.15×.
It allocates 3.20 million objects instead of 13.70 million. The digest of
every cleaned row matches plain roo.

Reproduce each version with:

```sh
cd bench
bundle exec ruby run.rb 5 roo-calamine --scenario each-headers,each-clean,cell-then-clean --fixture mixed-100k
```

## Other benchmarks

Detailed results beyond the default tables in [README.md](README.md#results).
Apple M5 Pro, Ruby 3.4.12, roo 4.0.0, calx 0.1.0, and xsv 1.4.1. Each timed
run starts a fresh process and includes opening the workbook. Values are
checked against roo before timing. Iteration rows use five runs, except
`each` on `mixed-10k.xlsx` and `each_row_streaming`, `each`, and `to_csv` on
`comments-formulas-10k.xlsx`: their roo-calamine and calx entries use 24 runs
on be4ab9b. Ratios use the published roo time and memory figures. Other
operations retain their earlier measurements.

The roo and roo-calamine results for `dates-10k.xlsx`'s `each`, `to_csv`, and
`formatted_value` include the date and time optimization. They are medians of nine fresh
processes per version, including workbook opening, on the same Ruby 3.4.12.
Value hashes match roo. The calx and xsv references keep their earlier results.
Repeat with `bundle exec ruby run.rb 9 --scenario csv,formatted_value,each --fixture dates-10k roo roo-calamine` from `bench`.

| File | Scenario | roo | roo + roo-calamine | Faster | Less memory | calx (reference) | Time vs calx | xsv (reference) | Flag |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| mixed-100k.xlsx | `Roo::Excelx.new(path)`, no read | 0.14 s, 118 MB | 3.1 ms, 42 MB | 45.9× | 2.8× | - | - | - |  |
| mixed-100k.xlsx | `Roo::Spreadsheet.open(path)`, no read | 80.4 ms, 113 MB | 2.8 ms, 41 MB | 29.2× | 2.7× | - | - | - |  |
| mixed-100k.xlsx | `Roo::Excelx.new(StringIO)`, no read | 77.8 ms, 118 MB | 3.2 ms, 50 MB | 24.4× | 2.4× | - | - | - |  |
| mixed-100k.xlsx | `Roo::Excelx.new(File)`, no read | 75.6 ms, 112 MB | 2.6 ms, 41 MB | 28.9× | 2.7× | - | - | - |  |
| mixed-100k.xlsx | `row(n)` over every row | 19.04 s, 3354 MB | 0.63 s, 175 MB | 30.0× | 19.2× | - | - | - |  |
| mixed-100k.xlsx | `column(n)` over every column | 19.19 s, 3282 MB | 0.58 s, 169 MB | 32.8× | 19.4× | - | - | - |  |
| mixed-100k.xlsx | `first_row` … `last_column`, `*_column_as_letter` | 17.20 s, 3275 MB | 0.46 s, 100 MB | 37.7× | 32.6× | - | - | - |  |
| mixed-100k.xlsx | `find(:all, conditions:)` | 17.85 s, 3312 MB | 0.76 s, 206 MB | 23.6× | 16.1× | - | - | - |  |
| mixed-100k.xlsx | `row_with(headers)` | 17.63 s, 3270 MB | 0.46 s, 100 MB | 38.6× | 32.5× | - | - | - |  |
| mixed-100k.xlsx | `each(headers: true)` | 20.27 s, 3300 MB | 0.73 s, 188 MB | 27.6× | 17.5× | - | - | - |  |
| mixed-100k.xlsx | `each(header_search:)` | 19.99 s, 3328 MB | 0.70 s, 188 MB | 28.6× | 17.6× | - | - | - |  |
| mixed-100k.xlsx | `each(key: "header", …)` | 17.42 s, 3322 MB | 0.52 s, 136 MB | 33.2× | 24.4× | - | - | - |  |
| mixed-100k.xlsx | `each(clean: true, headers: true)` | 20.42 s, 3278 MB | 2.95 s, 751 MB | 6.9× | 4.4× | - | - | - |  |
| mixed-100k.xlsx | one `cell`, then `each(clean: true, headers: true)` | 20.26 s, 3400 MB | 3.24 s, 722 MB | 6.2× | 4.7× | - | - | - |  |
| mixed-100k.xlsx | `parse(headers: true) { }` | 20.08 s, 3292 MB | 0.74 s, 216 MB | 27.0× | 15.2× | - | - | - |  |
| mixed-100k.xlsx | `each_row_streaming(offset: 5000)` | 12.68 s, 141 MB | 0.58 s, 62 MB | 22.0× | 2.2× | - | - | - |  |
| mixed-100k.xlsx | `each_row_streaming(max_rows: 1000)` | 0.21 s, 118 MB | 13.8 ms, 49 MB | 15.1× | 2.4× | - | - | - |  |
| mixed-100k.xlsx | `each_row_streaming(pad_cells: true)` | 13.22 s, 118 MB | 0.56 s, 51 MB | 23.7× | 2.3× | - | - | - |  |
| mixed-100k.xlsx | `each_row_streaming` from a `StringIO` | 13.32 s, 124 MB | 0.59 s, 60 MB | 22.7× | 2.1× | - | - | - |  |
| mixed-100k.xlsx | `sheet_for(...).cells` | 17.82 s, 3348 MB | 1.43 s, 647 MB | 12.5× | 5.2× | - | - | - |  |
| mixed-100k.xlsx | 1,000 `set`, then `each` | 19.18 s, 3354 MB | 2.04 s, 614 MB | 9.4× | 5.5× | - | - | - |  |
| mixed-100k.xlsx | 1,000 `set`, `reload`, then `each` | 38.58 s, 3341 MB | 2.14 s, 700 MB | 18.0× | 4.8× | - | - | - |  |
| mixed-100k.xlsx | `cell_max:` + `each` | 18.85 s, 3285 MB | 0.66 s, 175 MB | 28.4× | 18.8× | - | - | - |  |
| small.xlsx | `Roo::Excelx.new(path)`, no read | 1.6 ms, 40 MB | 0.7 ms, 40 MB | 2.3× | 1.0× | - | - | - | < 5× roo |
| small.xlsx | `Roo::Spreadsheet.open(path)`, no read | 1.7 ms, 40 MB | 0.8 ms, 40 MB | 2.1× | 1.0× | - | - | - | < 5× roo |
| small.xlsx | `Roo::Excelx.new(StringIO)`, no read | 1.5 ms, 40 MB | 0.7 ms, 40 MB | 2.1× | 1.0× | - | - | - | < 5× roo |
| small.xlsx | `Roo::Excelx.new(File)`, no read | 1.6 ms, 40 MB | 0.7 ms, 40 MB | 2.3× | 1.0× | - | - | - | < 5× roo |
| small.xlsx | `row(n)` over every row | 12.1 ms, 45 MB | 1.9 ms, 41 MB | 6.4× | 1.1× | - | - | - |  |
| small.xlsx | `column(n)` over every column | 12.0 ms, 45 MB | 1.9 ms, 41 MB | 6.5× | 1.1× | - | - | - |  |
| small.xlsx | `first_row` … `last_column`, `*_column_as_letter` | 11.6 ms, 45 MB | 1.7 ms, 41 MB | 6.7× | 1.1× | - | - | - |  |
| small.xlsx | `find(:all, conditions:)` | 11.8 ms, 45 MB | 2.0 ms, 41 MB | 5.9× | 1.1× | - | - | - |  |
| small.xlsx | `row_with(headers)` | 11.5 ms, 45 MB | 1.7 ms, 41 MB | 6.6× | 1.1× | - | - | - |  |
| small.xlsx | 10,000 random `celltype` | 20.7 ms, 46 MB | 9.0 ms, 41 MB | 2.3× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `formula` | 19.5 ms, 46 MB | 7.7 ms, 41 MB | 2.5× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `formula?` | 20.0 ms, 46 MB | 8.2 ms, 42 MB | 2.4× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | `formulas` | 11.4 ms, 45 MB | 2.3 ms, 41 MB | 4.9× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `excelx_type` | 19.6 ms, 45 MB | 7.9 ms, 41 MB | 2.5× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `excelx_value` | 19.5 ms, 46 MB | 7.8 ms, 41 MB | 2.5× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `formatted_value` | 26.8 ms, 46 MB | 15.7 ms, 43 MB | 1.7× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `excelx_format` | 20.9 ms, 46 MB | 10.9 ms, 42 MB | 1.9× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `font` | 21.2 ms, 46 MB | 10.4 ms, 42 MB | 2.0× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `empty?` | 18.4 ms, 46 MB | 8.5 ms, 42 MB | 2.2× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `hyperlink` | 15.8 ms, 46 MB | 5.8 ms, 41 MB | 2.7× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `hyperlink?` | 16.0 ms, 46 MB | 6.0 ms, 41 MB | 2.7× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `comment` | 16.0 ms, 46 MB | 5.9 ms, 41 MB | 2.7× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 10,000 random `comment?` | 16.2 ms, 46 MB | 6.1 ms, 41 MB | 2.7× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | `each_with_pagename` + `each` | 12.0 ms, 45 MB | 1.9 ms, 41 MB | 6.4× | 1.1× | - | - | - |  |
| small.xlsx | `sheet_for(...).cells` | 11.6 ms, 45 MB | 2.4 ms, 41 MB | 4.8× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | `to_s` | 14.6 ms, 46 MB | 5.3 ms, 42 MB | 2.8× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 1,000 `set`, then `each` | 13.1 ms, 45 MB | 3.7 ms, 41 MB | 3.6× | 1.1× | - | - | - | < 5× roo |
| small.xlsx | 1,000 `set`, `reload`, then `each` | 24.1 ms, 48 MB | 4.2 ms, 41 MB | 5.7× | 1.2× | - | - | - |  |
| small.xlsx | `sheets` | 1.7 ms, 40 MB | 0.8 ms, 40 MB | 2.1× | 1.0× | - | - | - | < 5× roo |
| small.xlsx | `sheet(n)` + one `cell` per sheet | 11.4 ms, 45 MB | 1.7 ms, 41 MB | 6.5× | 1.1× | - | - | - |  |
| small.xlsx | `dimensions` of every sheet | 1.7 ms, 40 MB | 1.0 ms, 40 MB | 1.7× | 1.0× | - | - | - | < 5× roo |
| mixed-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 1.30 s, 110 MB | 65.0 ms, 51 MB | 20.0× | 2.2× | 52.5 ms, 45 MB | 1.2× | 0.95 s, 50 MB | > 1.2× calx |
| mixed-10k.xlsx | `each` (one Array of values per row) | 1.33 s, 449 MB | 32.0 ms, 56 MB | 41.5× | 8.0× | 48.3 ms, 45 MB | 0.7× | 0.95 s, 50 MB |  |
| mixed-10k.xlsx | `to_csv` | 1.70 s, 448 MB | 77.6 ms, 51 MB | 21.9× | 8.7× | 66.4 ms, 45 MB | 1.2× | 0.97 s, 50 MB |  |
| mixed-10k.xlsx | 10,000 random `celltype` | 1.27 s, 448 MB | 58.9 ms, 50 MB | 21.7× | 9.0× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `formula` | 1.27 s, 448 MB | 57.6 ms, 50 MB | 22.0× | 8.9× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `formula?` | 1.27 s, 448 MB | 59.1 ms, 50 MB | 21.5× | 9.0× | - | - | - |  |
| mixed-10k.xlsx | `formulas` | 1.24 s, 456 MB | 0.10 s, 109 MB | 12.2× | 4.2× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `excelx_type` | 1.27 s, 448 MB | 57.6 ms, 50 MB | 22.0× | 9.0× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `excelx_value` | 1.27 s, 448 MB | 57.4 ms, 50 MB | 22.1× | 9.0× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `formatted_value` | 1.28 s, 450 MB | 66.2 ms, 51 MB | 19.3× | 8.8× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `excelx_format` | 1.27 s, 448 MB | 61.1 ms, 50 MB | 20.7× | 8.9× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `font` | 1.27 s, 448 MB | 60.4 ms, 50 MB | 21.1× | 8.9× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `empty?` | 1.27 s, 448 MB | 59.5 ms, 50 MB | 21.3× | 9.0× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `hyperlink` | 1.26 s, 448 MB | 51.0 ms, 49 MB | 24.8× | 9.1× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `hyperlink?` | 1.27 s, 448 MB | 51.1 ms, 49 MB | 24.7× | 9.1× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `comment` | 1.26 s, 448 MB | 50.8 ms, 49 MB | 24.9× | 9.1× | - | - | - |  |
| mixed-10k.xlsx | 10,000 random `comment?` | 1.27 s, 448 MB | 51.2 ms, 49 MB | 24.8× | 9.1× | - | - | - |  |
| mixed-10k.xlsx | `to_s` | 1.73 s, 487 MB | 0.44 s, 174 MB | 3.9× | 2.8× | - | - | - | < 5× roo |
| wide.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 0.35 s, 68 MB | 19.1 ms, 48 MB | 18.2× | 1.4× | 13.3 ms, 31 MB | 1.4× | 0.24 s, 50 MB | > 1.2× calx |
| wide.xlsx | `each` (one Array of values per row) | 0.37 s, 150 MB | 15.3 ms, 45 MB | 24.3× | 3.3× | 12.4 ms, 31 MB | 1.2× | 0.27 s, 50 MB | > 1.2× calx |
| wide.xlsx | `to_csv` | 0.45 s, 151 MB | 16.6 ms, 44 MB | 26.8× | 3.4× | 14.4 ms, 32 MB | 1.2× | 0.25 s, 50 MB |  |
| wide.xlsx | `row(n)` over every row | 0.32 s, 150 MB | 14.6 ms, 45 MB | 22.0× | 3.3× | - | - | - |  |
| wide.xlsx | `column(n)` over every column | 0.33 s, 150 MB | 17.1 ms, 45 MB | 19.3× | 3.3× | - | - | - |  |
| multi-sheet.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 2.70 s, 118 MB | 0.12 s, 51 MB | 21.8× | 2.3× | 0.11 s, 60 MB | 1.2× | 2.01 s, 51 MB |  |
| multi-sheet.xlsx | `each` (one Array of values per row) | 2.90 s, 788 MB | 0.12 s, 68 MB | 23.7× | 11.5× | 93.0 ms, 60 MB | 1.3× | 1.98 s, 51 MB | > 1.2× calx |
| multi-sheet.xlsx | `to_csv` | 3.56 s, 799 MB | 0.15 s, 59 MB | 23.2× | 13.5× | 0.13 s, 60 MB | 1.2× | 1.98 s, 51 MB |  |
| multi-sheet.xlsx | `Roo::Excelx.new(path)`, no read | 27.5 ms, 112 MB | 1.7 ms, 41 MB | 16.2× | 2.7× | - | - | - |  |
| multi-sheet.xlsx | `each_with_pagename` + `each` | 4.00 s, 966 MB | 0.17 s, 76 MB | 23.2× | 12.6× | - | - | - |  |
| multi-sheet.xlsx | `sheets` | 29.5 ms, 113 MB | 2.1 ms, 42 MB | 13.8× | 2.7× | - | - | - |  |
| multi-sheet.xlsx | `sheet(n)` + one `cell` per sheet | 3.75 s, 963 MB | 0.12 s, 58 MB | 30.5× | 16.4× | - | - | - |  |
| multi-sheet.xlsx | `dimensions` of every sheet | 30.3 ms, 113 MB | 19.7 ms, 42 MB | 1.5× | 2.7× | - | - | - | < 5× roo |
| links-merges-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 0.46 s, 170 MB | 49.2 ms, 60 MB | 9.4× | 2.8× | 13.0 ms, 31 MB | 3.8× | 0.27 s, 51 MB | > 1.2× calx |
| links-merges-10k.xlsx | `each` (one Array of values per row) | 0.37 s, 187 MB | 40.5 ms, 55 MB | 9.1× | 3.4× | 11.4 ms, 31 MB | 3.6× | 0.25 s, 50 MB | > 1.2× calx |
| links-merges-10k.xlsx | `to_csv` | 0.43 s, 188 MB | 51.5 ms, 52 MB | 8.3× | 3.6× | 14.7 ms, 31 MB | 3.5× | 0.26 s, 49 MB | > 1.2× calx |
| links-merges-10k.xlsx | 10,000 random `hyperlink` | 0.39 s, 183 MB | 42.1 ms, 51 MB | 9.3× | 3.6× | - | - | - |  |
| links-merges-10k.xlsx | 10,000 random `hyperlink?` | 0.38 s, 183 MB | 42.5 ms, 51 MB | 8.9× | 3.6× | - | - | - |  |
| links-merges-10k.xlsx | `sheet_for(...).hyperlinks` | 66.0 ms, 160 MB | 23.8 ms, 48 MB | 2.8× | 3.3× | - | - | - | < 5× roo |
| links-merges-10k.xlsx | `expand_merged_ranges: true` + `each` | 0.39 s, 188 MB | 69.4 ms, 66 MB | 5.6× | 2.8× | - | - | - |  |
| links-merges-10k.xlsx | `no_hyperlinks: true` + `each` | 0.32 s, 172 MB | 19.2 ms, 45 MB | 16.7× | 3.8× | - | - | - |  |
| comments-formulas-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 0.36 s, 73 MB | 22.3 ms, 51 MB | 16.1× | 1.4× | 14.9 ms, 32 MB | 1.5× | 0.27 s, 51 MB | > 1.2× calx |
| comments-formulas-10k.xlsx | `each` (one Array of values per row) | 0.29 s, 165 MB | 13.3 ms, 46 MB | 21.9× | 3.5× | 14.7 ms, 32 MB | 0.9× | 0.27 s, 51 MB |  |
| comments-formulas-10k.xlsx | `to_csv` | 0.37 s, 164 MB | 12.9 ms, 45 MB | 28.7× | 3.6× | 17.1 ms, 32 MB | 0.8× | 0.28 s, 51 MB |  |
| comments-formulas-10k.xlsx | 10,000 random `formula` | 0.29 s, 163 MB | 27.8 ms, 46 MB | 10.5× | 3.5× | - | - | - |  |
| comments-formulas-10k.xlsx | 10,000 random `formula?` | 0.32 s, 163 MB | 29.4 ms, 46 MB | 11.0× | 3.5× | - | - | - |  |
| comments-formulas-10k.xlsx | `formulas` | 0.28 s, 163 MB | 29.1 ms, 61 MB | 9.8× | 2.7× | - | - | - |  |
| comments-formulas-10k.xlsx | 10,000 random `comment` | 0.33 s, 165 MB | 31.4 ms, 52 MB | 10.4× | 3.2× | - | - | - |  |
| comments-formulas-10k.xlsx | 10,000 random `comment?` | 0.29 s, 165 MB | 31.5 ms, 52 MB | 9.3× | 3.1× | - | - | - |  |
| comments-formulas-10k.xlsx | `comments` | 15.2 ms, 70 MB | 8.3 ms, 48 MB | 1.8× | 1.5× | - | - | - | < 5× roo |
| comments-formulas-10k.xlsx | `labels` | 6.0 ms, 64 MB | 1.9 ms, 41 MB | 3.2× | 1.6× | - | - | - | < 5× roo |
| comments-formulas-10k.xlsx | `images` | 5.9 ms, 64 MB | 1.8 ms, 40 MB | 3.3× | 1.6× | - | - | - | < 5× roo |
| dates-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 0.38 s, 76 MB | 59.1 ms, 51 MB | 6.5× | 1.5× | 49.2 ms, 33 MB | 1.2× | 0.42 s, 43 MB | > 1.2× calx |
| dates-10k.xlsx | `each` (one Array of values per row) | 0.37 s, 188 MB | 46.3 ms, 55 MB | 7.9× | 3.4× | 46.1 ms, 33 MB | 1.0× | 0.42 s, 43 MB |  |
| dates-10k.xlsx | `to_csv` | 0.51 s, 193 MB | 51.8 ms, 51 MB | 9.8× | 3.8× | 73.5 ms, 33 MB | 0.7× | 0.44 s, 43 MB |  |
| dates-10k.xlsx | `to_yaml` | 0.54 s, 197 MB | 0.11 s, 70 MB | 4.9× | 2.8× | - | - | - | < 5× roo |
| dates-10k.xlsx | `to_xml` | 0.76 s, 228 MB | 97.9 ms, 80 MB | 7.7× | 2.8× | - | - | - |  |
| dates-10k.xlsx | `to_matrix` | 0.38 s, 190 MB | 54.9 ms, 56 MB | 6.9× | 3.3× | - | - | - |  |
| dates-10k.xlsx | 10,000 random `celltype` | 0.34 s, 183 MB | 34.5 ms, 50 MB | 9.8× | 3.6× | - | - | - |  |
| dates-10k.xlsx | 10,000 random `excelx_type` | 0.34 s, 183 MB | 33.2 ms, 50 MB | 10.1× | 3.6× | - | - | - |  |
| dates-10k.xlsx | 10,000 random `excelx_value` | 0.34 s, 183 MB | 33.0 ms, 50 MB | 10.2× | 3.6× | - | - | - |  |
| dates-10k.xlsx | 10,000 random `formatted_value` | 0.43 s, 200 MB | 39.7 ms, 51 MB | 10.8× | 3.9× | - | - | - |  |
| dates-10k.xlsx | 10,000 random `excelx_format` | 0.34 s, 184 MB | 36.3 ms, 50 MB | 9.3× | 3.6× | - | - | - |  |
| rich-text-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 1.55 s, 288 MB | 55.1 ms, 61 MB | 28.2× | 4.7× | 37.9 ms, 34 MB | 1.5× | 0.85 s, 86 MB | > 1.2× calx |
| rich-text-10k.xlsx | `each` (one Array of values per row) | 0.92 s, 345 MB | 52.7 ms, 56 MB | 17.6× | 6.1× | 36.1 ms, 34 MB | 1.5× | 0.85 s, 86 MB | > 1.2× calx |
| rich-text-10k.xlsx | `to_csv` | 1.01 s, 344 MB | 50.7 ms, 58 MB | 20.0× | 5.9× | 37.2 ms, 34 MB | 1.4× | 0.85 s, 87 MB | > 1.2× calx |
| rich-text-10k.xlsx | `disable_html_wrapper: true` + `each` | 0.48 s, 314 MB | 42.7 ms, 47 MB | 11.3× | 6.6× | - | - | - |  |
| number-text-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 1.36 s, 112 MB | 67.6 ms, 51 MB | 20.1× | 2.2× | 53.4 ms, 45 MB | 1.3× | 0.98 s, 50 MB | > 1.2× calx |
| number-text-10k.xlsx | `each` (one Array of values per row) | 1.37 s, 451 MB | 63.9 ms, 56 MB | 21.5× | 8.0× | 46.0 ms, 45 MB | 1.4× | 0.97 s, 50 MB | > 1.2× calx |
| number-text-10k.xlsx | `to_csv` | 1.73 s, 451 MB | 79.4 ms, 52 MB | 21.8× | 8.6× | 67.6 ms, 45 MB | 1.2× | 1.00 s, 50 MB |  |
| fallback-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 1.30 s, 110 MB | 1.31 s, 47 MB | 1.0× | 2.3× | 52.3 ms, 45 MB | 25.0× | 0.95 s, 50 MB | < 5× roo, > 1.2× calx |
| fallback-10k.xlsx | `each` (one Array of values per row) | 1.33 s, 449 MB | 1.29 s, 392 MB | 1.0× | 1.1× | 45.3 ms, 45 MB | 28.5× | 0.95 s, 50 MB | < 5× roo, > 1.2× calx |
| fallback-10k.xlsx | `to_csv` | 1.70 s, 448 MB | 1.39 s, 394 MB | 1.2× | 1.1× | 66.4 ms, 45 MB | 21.0× | 0.97 s, 50 MB | < 5× roo, > 1.2× calx |
| fallback-dtd-10k.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 1.33 s, 110 MB | 1.32 s, 48 MB | 1.0× | 2.3× | 52.8 ms, 45 MB | 25.1× | 0.98 s, 51 MB | < 5× roo, > 1.2× calx |
| fallback-dtd-10k.xlsx | `each` (one Array of values per row) | 1.34 s, 448 MB | 1.30 s, 394 MB | 1.0× | 1.1× | 45.8 ms, 45 MB | 28.4× | 0.96 s, 50 MB | < 5× roo, > 1.2× calx |
| fallback-dtd-10k.xlsx | `to_csv` | 1.71 s, 448 MB | 1.49 s, 393 MB | 1.1× | 1.1× | 68.2 ms, 45 MB | 21.8× | 1.02 s, 50 MB | < 5× roo, > 1.2× calx |
| large-1m.xlsx | `Roo::Spreadsheet.open` + `each_row_streaming` | 130.46 s, 118 MB | 5.75 s, 51 MB | 22.7× | 2.3× | 5.21 s, 1511 MB | 1.1× | 95.56 s, 51 MB |  |
| large-1m.xlsx | values only: `Roo::Calamine.each_row_values` | 139.54 s, 119 MB | 4.42 s, 47 MB | 31.6× | 2.5× | 5.36 s, 1511 MB | 0.8× | 100.57 s, 67 MB |  |
| csv-10k.csv | `each` (one Array of values per row) | 0.17 s, 88 MB | 0.17 s, 89 MB | 1.0× | 1.0× | - | - | - | < 5× roo |
| csv-10k.csv | 10,000 random `cell(row, col)` | 0.11 s, 82 MB | 0.12 s, 83 MB | 1.0× | 1.0× | - | - | - | < 5× roo |
| csv-10k.csv | `to_csv` | 0.49 s, 90 MB | 0.50 s, 89 MB | 1.0× | 1.0× | - | - | - | < 5× roo |

## Flagged

A row is flagged when roo-calamine is less than 5× faster than roo, or more
than 1.2× slower than calx. 53 of 133 rows are; the reasons:

- **roo's own code.** These reads still run roo's code, so roo-calamine only
  saves the time it takes to open the workbook:
  - `dimensions`, `comments`, `labels`, and `images` parse their XML with
    Nokogiri, as roo does;
  - `fallback-10k.xlsx` (a non-UTF-8 encoding) and `fallback-dtd-10k.xlsx` (a
    DTD with an entity) are worksheets roo-calamine hands back to roo;
  - roo-calamine does not read CSV; these rows show it adds no cost.
- **The 100-row file.** Each run takes under 30 ms, mostly opening the file and
  roo's Ruby code around each call. The same 10,000 single-cell reads are
  19–25× faster on `mixed-10k.xlsx`.
- **`to_s`** is `cells.inspect`: 97% of its time is Ruby inspecting every
  cell, which roo's output requires.
- **Non-text links, temporal values, and merged ranges.** Linked text now uses the native
  cell builder. Non-text links still use roo's constructors; temporal values
  keep roo's rounding and object types. `expand_merged_ranges` builds the
  sheet whole:
  - `links-merges-10k.xlsx` is about 8× faster than roo for `each`, but
    3.8× slower than calx, which returns plain values without link targets;
  - on `dates-10k.xlsx`, `to_yaml` is 4.9×;
  - `sheet_for(...).hyperlinks` is 2.8× faster than roo. Its 23.8 ms still
    includes validating the sheet XML and building the coordinate Hash.

- **roo's objects.** Other flagged reads retain roo's cells, rich-text HTML,
  and value rules. `each_row_streaming` on `comments-formulas-10k.xlsx` is
  1.5× slower than calx: it constructs cells with coordinates and formula
  text, while calx yields plain values.

The default table's flags have the same causes: the 100-row file, and calx's
plain values against roo's cells.

## Not covered

- `set` followed by `to_csv`: roo 4.0.0 raises `NoMethodError`, because `set`
  stores a `Roo::Excelx::Cell` that has no `empty?`.
- A sheet where some cell would raise: every read raises under roo too, so
  there is nothing to time.
- `.ods` and `.xls`: the bench cannot write them, and `.xls` needs roo-xls.

## Comparing with calx

Calx returns plain values without roo cells or hyperlink targets. It has no
hyperlink lookup, merged-range expansion, or `to_csv` API. Its CSV reference
reads rows and joins values with commas, without quoting fields.

Calx loads the whole XLSX sheet before yielding rows. roo-calamine streams
with bounded memory. Compare their outputs with care: `each_row_streaming` returns roo cells,
while `each_row_values` returns their values without those wrappers.
