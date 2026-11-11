# roo-calamine

Make [roo](https://github.com/roo-rb/roo) read `.xlsx` and `.xlsm` files
**12–36× faster** on large sheets, with **2–31× less memory** and
identical results. Your roo code does not change.

```ruby
# Gemfile
gem "roo"
gem "roo-calamine"
```

```ruby
xlsx = Roo::Spreadsheet.open("orders.xlsx") # or Roo::Excelx.new("orders.xlsx")
xlsx.each_row_streaming { |row| ... }        # read by the native reader
xlsx.parse(headers: true)                    # this too
```

> **Unofficial.** roo-calamine is not affiliated with or endorsed by roo-rb or
> the roo maintainers.

## Install

Add the gem next to roo, as above. Bundler requires it on `Bundler.require`;
without Bundler, `require "roo/calamine"`.

| | |
|---|---|
| Ruby | CRuby 3.3, 3.4, 4.0 |
| roo | `~> 4.0` |
| Precompiled | Linux x86_64/aarch64 (glibc, musl), macOS x86_64/arm64, Windows x64 |
| Other platforms | the source gem compiles with Rust 1.86 or later |
| JRuby | installs without Rust and stays inert: roo reads every file itself |

## Benchmarks

Median wall time and peak RSS; each run is a fresh process and includes
opening the workbook. Apple M5 Pro, Ruby 3.4.12, roo 4.0.0, calx 0.1.0;
every column is the median of 5 runs on 65bd561.

| File | Scenario | roo | roo + roo-calamine | Speedup | calx (reference) |
|---|---|---:|---:|---:|---:|
| 100k rows × 20 mixed columns | `each_row_streaming` | 13.49 s, 119 MB | 0.59 s, 51 MB | 22.9× | 0.52 s, 178 MB |
| | `parse(headers: true)` | 19.61 s, 3300 MB | 0.76 s, 281 MB | 25.8× | 0.71 s, 290 MB |
| | `each` | 18.81 s, 3282 MB | 0.63 s, 174 MB | 29.9× | 0.45 s, 178 MB |
| | 10,000 random `cell(row, col)` | 17.25 s, 3272 MB | 0.48 s, 104 MB | 35.9× | 0.51 s, 250 MB |
| | `to_csv` | 21.72 s, 3369 MB | 0.76 s, 118 MB | 28.6× | 0.66 s, 178 MB |
| | values only: `each_row_values` | 13.43 s, 118 MB | 0.43 s, 47 MB | 31.2× | 0.52 s, 178 MB |
| 100k rows × 10 shared strings | `each_row_streaming` | 4.92 s, 121 MB | 0.26 s, 52 MB | 18.9× | 0.24 s, 114 MB |
| | `parse(headers: true)` | 6.02 s, 1630 MB | 0.37 s, 117 MB | 16.3× | 0.33 s, 208 MB |
| | `each` | 5.66 s, 1631 MB | 0.31 s, 74 MB | 18.3× | 0.20 s, 113 MB |
| | 10,000 random `cell(row, col)` | 4.93 s, 1602 MB | 0.24 s, 70 MB | 20.5× | 0.22 s, 167 MB |
| | `to_csv` | 7.17 s, 1619 MB | 0.28 s, 88 MB | 25.6× | 0.22 s, 114 MB |
| | values only: `each_row_values` | 5.12 s, 120 MB | 0.19 s, 49 MB | 26.9× | 0.24 s, 113 MB |
| 100k rows × 10 unique strings | `each_row_streaming` | 10.27 s, 933 MB | 0.47 s, 250 MB | 21.9× | 0.38 s, 152 MB |
| | `parse(headers: true)` | 9.07 s, 2327 MB | 0.77 s, 251 MB | 11.8× | 0.47 s, 206 MB |
| | `each` | 8.80 s, 2262 MB | 0.72 s, 205 MB | 12.2× | 0.34 s, 152 MB |
| | 10,000 random `cell(row, col)` | 8.49 s, 2213 MB | 0.56 s, 207 MB | 15.2× | 0.36 s, 206 MB |
| | `to_csv` | 10.40 s, 2366 MB | 0.64 s, 225 MB | 16.2× | 0.37 s, 153 MB |
| | values only: `each_row_values` | 10.32 s, 925 MB | 0.37 s, 160 MB | 27.9× | 0.37 s, 152 MB |
| 100 rows × 20 mixed columns | `each_row_streaming` | 15.9 ms, 45 MB | 2.5 ms, 41 MB | 6.4× | 0.7 ms, 27 MB |
| | `parse(headers: true)` | 12.8 ms, 45 MB | 2.2 ms, 41 MB | 5.8× | 0.7 ms, 27 MB |
| | `each` | 12.4 ms, 45 MB | 2.1 ms, 41 MB | 5.9× | 0.6 ms, 27 MB |
| | 10,000 random `cell(row, col)` | 19.4 ms, 46 MB | 7.7 ms, 41 MB | 2.5× | 1.5 ms, 27 MB |
| | `to_csv` | 15.3 ms, 46 MB | 2.2 ms, 40 MB | 7.0× | 0.8 ms, 27 MB |
| | values only: `each_row_values` | 16.2 ms, 45 MB | 2.1 ms, 41 MB | 7.7× | 0.7 ms, 27 MB |

<details>
<summary>How the benchmark runs</summary>

- Before timing, the benchmark checks that roo and roo-calamine read the same
  values.
- For the values-only rows, plain roo runs the
  `each_row_streaming(pad_cells: true)` code shown under
  [Values only](#values-only).
- **calx** is [calx](https://github.com/7a6163/calx) 0.1.0, a Rust reader built
  on calamine that skips roo altogether. It shows how fast a reader without
  roo's rules can go, but it is not a drop-in replacement:
  - a whole number is an Integer whatever the cell's text and format, a date
    with a time is a local `Time`, and there are no roo cells or `Roo::Link`s;
  - it loads the whole sheet before it yields the first row;
  - it runs `Calx.each_row` for the streaming, `each`, and values-only rows,
    `Calx.read` for the others, and for `to_csv` `Calx.each_row` with each
    row joined by commas, without quoting.

```sh
cd bench && bundle install
bundle exec ruby run.rb                  # each scenario 3–10 times, stopping after 10 s
bundle exec ruby run.rb 5 roo-calamine   # a fixed run count, and only the libraries named
```

</details>

### Whole-sheet formatters

On `mixed-100k.xlsx`, a single fresh-process run per entry, including opening
the workbook, on the same Apple M5 Pro and Ruby 3.4.12. Before is roo-calamine
on 11f2a02. These are single measurements, separate from the medians above.

| Call | roo 4.0.0 | Before | Now | Speedup over roo |
|---|---:|---:|---:|---:|
| `to_yaml` | 24.97 s | 8.21 s | 1.30 s | 19.2× |
| `to_xml` | 53.29 s | 38.21 s | 1.05 s | 50.6× |
| `to_matrix` | 20.70 s | 2.30 s | 0.69 s | 30.0× |

The three full-output SHA-256 digests match roo. YAML and XML compare the
exact bytes; Matrix compares its inspected values. Native formatting uses
roo 4.0.0's records and preserves values and cells changed in place. Once
the public cells hash is materialized, roo formats the output itself.

Run one scenario from the repository root after generating the fixtures:

```sh
BUNDLE_GEMFILE=bench/Gemfile bundle exec ruby -Ilib bench/scenario.rb roo-calamine yaml bench/fixtures/mixed-100k.xlsx
```

Use `roo` for the reference, `xml` or `matrix` for the other formatters, and
append `digest` to check output in a separate run.

## Values only

If you only need values, `Roo::Calamine.each_row_values` skips roo's cell
objects. It is roo-calamine's own API, not roo's, and using it changes nothing
in what roo does.

```ruby
Roo::Calamine.each_row_values("orders.xlsx", sheet: "Orders") do |values|
  values # => [1001, "Ada", #<Date: 2024-05-01>, 19.5, nil, true]
end
```

Each row equals, value for value, what this roo code yields:

```ruby
Roo::Excelx.new("orders.xlsx").each_row_streaming(sheet: "Orders", pad_cells: true) do |cells|
  cells.map { |cell| cell&.value }
end
```

<details>
<summary>Arguments and value rules</summary>

- **Arguments:** a path or an IO; `sheet:` is a name or index (default: the
  first sheet); other keywords are `Roo::Excelx.new` options.
- **Return:** an Enumerator without a block. It closes the workbook when it is
  done.
- **Rows:** one entry per column from A to the row's last cell, with `nil`
  where there is no cell.
- **Values** follow roo's rules: Integer or Float by the written text and
  number format; Date, DateTime and Time by the style; roo's shared strings
  (HTML for rich text); `Roo::Link` for hyperlinks; error text such as
  `"#DIV/0!"`.
- With the reader off, or on JRuby, it reads through roo and yields the same
  rows.

</details>

## Turning it off

```ruby
Roo::Calamine.enabled = false # from the next sheet read on
```

```sh
ROO_CALAMINE=0 bundle exec rails server # also: false, off
```

Either one makes roo read exactly as it does without the gem.
`Roo::Calamine.enabled?` tells whether the next read uses the native reader,
and `Roo::Calamine.available?` whether the extension is loaded.

## Correctness

roo is the oracle: every check takes roo's own output as the expected result.

| Suite | What it compares with roo |
|---|---|
| `rake parity` | roo 4.0.0's own Excelx tests and specs, run with and without roo-calamine |
| `test/roo_fixtures_test.rb` | all 84 `.xlsx`/`.xlsm` files in roo's `test/files`, cell by cell |
| `test/edge_cases_test.rb` | numbers, dates and times under every style, rich text, merged ranges, formulas, errors, and more |
| `test/shared_strings_test.rb` | `SharedStrings#to_a` and `#to_html`, read in either order |
| `test/to_csv_test.rb` | the bytes `to_csv` returns and writes |

<details>
<summary>What the suites cover</summary>

- **Every roo fixture:** class, value, type, formatted value, and every
  instance variable of each cell, across open options, streaming options, and
  the `Roo::Base` API: `parse` and `each` under nine option sets, with and
  without a block, the single-cell reads, and whether `cells`, built after
  `each` and `parse`, holds the very values they returned.
- **Edge cases:** Integer vs Float, random numbers, dates, and times under
  every number and date style; number text only roo's constructors read; the
  1904 date system; merged ranges; rich text; inline strings; shared formulas;
  error cells; namespaces; `parse` and `each` after `set` or with a block that
  changes the workbook.
- **Shared strings:** `_x000D_` across text, CDATA, and references; `t`
  elements outside runs; tag order; local names under `/sst/si`; line endings;
  and each case it leaves to roo. The fixture test does the same for every roo
  fixture.
- **CSV:** every cell type, separator, sheet, and file argument; cells out of
  order, repeated, or merged; keys written into the cells hash; values and
  cells changed in place before it; `set` before it; and the file roo leaves
  behind when a cell raises.
- **Values only:** both the fixture and edge-case suites compare
  `Roo::Calamine.each_row_values` with the values of roo's
  `each_row_streaming(pad_cells: true)`, with the native reader on and off.

</details>

### Known differences

These are by design.

<details>
<summary>Overrides of roo's private methods</summary>

- Under roo 4.0.0, overrides of roo's private shared-string parsing
  (`SharedStrings#extract_shared_strings`, `#extract_html`,
  `#fix_invalid_shared_strings`, `#create_html`) are bypassed while the reader
  is on. Other roo versions build their shared strings themselves.
- Overrides of roo's private cell parsing (`SheetDoc#cell_from_xml`,
  `#extract_cells`, `#extract_hyperlinks`, `#expand_merged_ranges`,
  `Cell.cell_class`) are bypassed while the reader is on. So are:
  - a `Roo::Excelx` subclass's `#cell` or `#each` inside `parse` and `each`
    with options;
  - its `#cell`, `#celltype`, `#empty?`, or roo's private `#cell_to_csv`
    inside `to_csv`;
  - redefinitions of the [single-cell reads](#reading-cells) in `Roo::Excelx`
    or `Roo::Excelx::Sheet` themselves (subclasses work);
  - under roo 4.0.0, an override of `initialize` on a `Cell` class or on
    `Roo::Excelx::Coordinate`.

</details>

<details>
<summary>Behavior and performance</summary>

- **Building `Sheet#cells` late costs more.** Building the whole hash (`#set`,
  `clean: true`, …) after a read that does not need it (a single-cell read,
  `#last_row`, `#row`, `#each`, `#parse`) costs more than building it from the
  start, because the sheet is parsed before any cell is built rather than
  alongside.
- **Non-UTF-8 text set by hand.** A cell value set to non-ASCII text in an
  encoding other than UTF-8 can make `to_csv` raise
  `Encoding::CompatibilityError`, where roo writes it, when its line also
  holds non-ASCII UTF-8 text. Cells read from a file are always UTF-8.
- **Writes straight into `cells`.** On a [sheet built whole](#reading-cells),
  inside an `each` block without options, a write straight into
  `sheet_for(...).cells` (not through `set`) does not show in later rows.
- **Broken XML.** On XML that breaks partway, `each_row_streaming` raises the
  same `Nokogiri::XML::SyntaxError` as roo but yields a few more rows before
  it.
- **Errors come when a sheet is read.** A workbook opened from a path leaves
  its worksheets in the zip, so a worksheet whose compressed data is broken,
  or larger than the zip says, raises roo's error when it is read rather than
  in `Roo::Excelx.new`. The paths in `#sheet_files` exist only once roo's own
  code has read them.
- **Streaming from an IO** uses about 20 MB more peak memory than roo. A
  workbook opened from a path uses less.

</details>

## How it works

A Rust extension parses worksheet XML with
[quick-xml](https://github.com/tafia/quick-xml) on a background thread and
builds the same `Roo::Excelx::Cell` objects roo would. In short:

- **Worksheets:** it prepends `Roo::Excelx::SheetDoc` and replaces the two
  places roo reads worksheet XML: `#cells` (behind `#cell`, `#row`, `#each`,
  `#parse`, `#to_csv`, …) and `#each_row_streaming`.
- **Lazy cells:** the parsed sheet lives in compact native records, and a cell
  is built only when something reads it.
- **Shared strings:** `sharedStrings.xml` is read natively, and rich-text HTML
  is built only when needed.
- **Opening:** worksheets stay in the zip until they are read, instead of
  being extracted to files.
- **Fallback:** XML the reader cannot reproduce exactly the way Nokogiri does
  goes back to roo.

Everything else stays roo's code: styles, number formats, the base date,
comments, images, and every open option. It works with
`Roo::Spreadsheet.open`, `Roo::Excelx.new`, and IO streams.

<details>
<summary>The details</summary>

### Building cells

Under roo 4.0.0 the extension sets each cell's instance variables the way
roo's constructors do, without running them. Cells with a hyperlink, number
text that is not plainly written, and every cell under other roo versions go
through roo's own constructors.

### Reading cells

- **Single-cell reads** (`#cell`, `#celltype`, `#formula`, `#excelx_type`,
  `#excelx_value`, `#formatted_value`, `#excelx_format`, `#font`, `#empty?`)
  and `#first_row` … `#last_column` build only the cells they read, the first
  time each is read.
- **`#row`, `#column`, `#each`, and `#parse`** read values straight from the
  records, and build a cell only for a
  value that needs one (a hyperlink, a date with a time, a time).
- The records keep each value they hand out, so a cell built later holds that
  very object, as in roo.
- **Anything that needs the whole `Sheet#cells` hash** (`#set`, `clean: true`,
  …) builds it from the records, keeping the
  cells and values already handed out.
- A sheet where some cell would raise, or one opened with
  `expand_merged_ranges`, is **built whole**, as before.

roo finds a sheet's hyperlinks, and its merged ranges under
`expand_merged_ranges`, with XPath on a Nokogiri document of the whole sheet.
The extension reads those `hyperlink` and `mergeCell` elements alone, and
copies of roo's loops turn them into the same hyperlinks and merged cells.

### `each` and `parse`

- **With options** (`headers`, `header_search`, header queries, `clean`),
  roo-calamine prepends `Roo::Excelx#parse` and `#each`. They keep roo's
  header handling and build each row's Hash with less work than roo's one
  `#cell` call per value. `parse` without a block builds each Hash from the
  records, or, on a sheet built whole, groups the cells by row in one pass.
  `each`, and `parse` with a block, resolve the current sheet on each row
  and read its records until its `cells` hash is built. A block that calls
  `set` or `reload`, or changes `default_sheet`, sees the change from the
  next row on. `clean: true` builds the cells before iterating, as in roo.
- **Without options**, `each` (behind `each_with_index`, `to_a`, and `parse`
  without options) is roo's own loop over `#row`, so a block that calls `set`
  or `reload`, or changes `default_sheet`, sees the change from the next row
  on. On a sheet built whole of 3,000 rows or more, it reads the rows past the
  first 1,000 from an index built in one pass over the sheet's cells, and
  hands back to roo's lookups once the block calls `set` or `reload`, or
  changes `default_sheet`.

### `to_csv`

Under roo 4.0.0 roo-calamine replaces the private writer behind `#to_csv`.
roo writes every slot from A1 to the last row and column through `#empty?`,
`#cell`, and `#celltype`, which look the sheet and the cell up again for each
slot.

- **While the cells are in the native records**, the extension writes the CSV
  from them in row and column order, without building the `cells` hash. It
  formats strings and booleans itself and hands numbers and dates to the same
  Ruby formatting roo uses. It builds a cell only for a field that needs one
  (a hyperlink, a date with a time, a time), and asks the cells and values
  already handed out, which a caller may have changed.
- **Otherwise** it walks the `cells` hash once, in row and column order, and
  writes a line per row, formatting each cell as roo's `#cell_to_csv` does.
  When the hash is not in that order (merged ranges expanded, rows out of
  order in the XML, or keys written into it), it sorts the keys first.
- Under other roo versions, roo writes the CSV itself.

### Shared strings

roo-calamine prepends `Roo::Excelx::SharedStrings#to_a` and `#to_html`. The
extension reads `sharedStrings.xml` with quick-xml and builds the requested
array the way roo does, `_x000D_` fix and rich-text HTML included.

- HTML stays in a compact native table until a cell needs rich text or
  something calls `#to_html`. `disable_html_wrapper` skips that table until
  something asks for it.
- Public arrays hold the same mutable strings the reader uses.
- Reading `#to_html` before `#to_a` reads the file again for the plain array.
- If the file uses XML that Nokogiri reads differently (see
  [Fallback to roo](#fallback-to-roo)), or a `vertAlign` without `val` (roo
  raises on it), roo builds the arrays itself.

### Opening a workbook

roo extracts every worksheet of a workbook to a temporary directory when it
opens it. With the reader on, a workbook opened from a path (not an IO) is
copied to that directory instead, and roo opens the copy.

- **Worksheets stay in the copied zip.** The reader inflates the one it reads,
  using a separate thread for large entries. A worksheet is extracted only
  when roo's own code reads it (`#dimensions`, or XML the reader hands back to
  roo).
- **Opening a large workbook** writes the .xlsx once rather than every
  worksheet inflated, and skips the memory roo's extraction leaves behind.
- **The files roo still extracts** while it opens the copy
  (`sharedStrings.xml`, styles, comments, relationships, images) are inflated
  by the extension, not rubyzip. That keeps a workbook with many unique
  strings from growing by several times its `sharedStrings.xml`.
- **An entry the extension cannot extract**, such as an encrypted one or one
  that inflates past its size, is left to rubyzip, which raises or warns as in
  roo.
- As with roo, a workbook reads the same after its file is changed or deleted.

### Fallback to roo

When a sheet uses XML the reader does not reproduce exactly the way Nokogiri
does (a non-UTF-8 encoding, a DTD, custom entities, or malformed XML), it
hands the sheet back to roo. A stream that hits such a spot continues in roo
from the next row. Shared strings follow the same rules, and also go back to
roo when the part holds a processing instruction or a `<t>` holds markup
other than text and CDATA.

</details>

## Development

```sh
bundle install
bundle exec rake compile   # build the extension
bundle exec rake test      # unit, fixture, and edge-case tests
bundle exec rake parity    # roo 4.0.0's own suites, with and without roo-calamine
bundle exec rubocop
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

## License

MIT. See [LICENSE](LICENSE).
