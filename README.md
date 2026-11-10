# roo-calamine

Make [roo](https://github.com/roo-rb/roo) read `.xlsx` and `.xlsm` files
8–22× faster on large sheets, with identical results and 4–14× less memory
outside streaming. Your roo code does not change.

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

Median of 3 runs on an Apple M5 Pro, Ruby 3.4.12, each run a fresh process
(5 runs for the small file). A run starts after `require`, so it includes
opening the workbook. Memory is peak RSS.

| File | Scenario | roo | roo + roo-calamine | Speedup | calx (reference) |
|---|---|---:|---:|---:|---:|
| 100k rows × 20 mixed columns | `each_row_streaming` | 14.49 s, 119 MB | 0.90 s, 133 MB | 16.2× | 0.58 s, 178 MB |
| | `parse(headers: true)` | 21.13 s, 3306 MB | 2.16 s, 828 MB | 9.8× | 0.75 s, 283 MB |
| | `each` | 20.23 s, 3281 MB | 1.83 s, 671 MB | 11.0× | 0.46 s, 178 MB |
| | 10,000 random `cell(row, col)` | 18.64 s, 3274 MB | 0.86 s, 235 MB | 21.6× | 0.50 s, 249 MB |
| | values only: `each_row_values` | 14.86 s, 118 MB | 0.79 s, 133 MB | 18.8× | 0.55 s, 178 MB |
| 100k rows × 10 shared strings | `each_row_streaming` | 5.37 s, 121 MB | 0.41 s, 133 MB | 13.3× | 0.25 s, 113 MB |
| | `parse(headers: true)` | 6.44 s, 1630 MB | 0.83 s, 397 MB | 7.8× | 0.34 s, 208 MB |
| | `each` | 5.96 s, 1630 MB | 0.77 s, 322 MB | 7.7× | 0.21 s, 113 MB |
| | 10,000 random `cell(row, col)` | 5.33 s, 1602 MB | 0.45 s, 160 MB | 11.9× | 0.23 s, 167 MB |
| | values only: `each_row_values` | 5.21 s, 121 MB | 0.36 s, 166 MB | 14.3× | 0.24 s, 113 MB |
| 100 rows × 20 mixed columns | `each_row_streaming` | 16.6 ms, 45 MB | 3.4 ms, 41 MB | 4.8× | 0.7 ms, 27 MB |
| | `parse(headers: true)` | 13.0 ms, 45 MB | 3.8 ms, 41 MB | 3.4× | 0.8 ms, 27 MB |
| | `each` | 12.7 ms, 45 MB | 4.0 ms, 41 MB | 3.2× | 0.6 ms, 26 MB |
| | 10,000 random `cell(row, col)` | 19.8 ms, 46 MB | 10.4 ms, 42 MB | 1.9× | 1.5 ms, 27 MB |
| | values only: `each_row_values` | 16.8 ms, 46 MB | 3.1 ms, 41 MB | 5.4× | 0.7 ms, 27 MB |

For the values-only rows, plain roo runs the `each_row_streaming(pad_cells:
true)` code [below](#values-only). Before timing, the benchmark checks that
roo and roo-calamine read the same values.

The calx column is [calx](https://github.com/7a6163/calx) 0.1.0, a Rust
reader built on calamine that skips roo altogether: `Calx.each_row` for the
streaming, `each`, and values-only rows, and `Calx.read` for the others. It
shows how fast a reader without roo's rules can go. It is not a drop-in
replacement: its values follow its own rules (a whole number is an Integer
whatever the cell's text and format, a date with a time is a local `Time`,
and there are no roo cells or `Roo::Link`s), and it loads the whole sheet
before it yields the first row.

```sh
cd bench && bundle install && bundle exec ruby run.rb # optional: number of runs
```

## Values only

If you only need values, `Roo::Calamine.each_row_values` skips roo's cell
objects. It is roo-calamine's own API, not roo's, and using it changes
nothing in what roo does.

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

So the values follow roo's rules: Integer or Float by the written text and
number format, Date, DateTime and Time by the style, roo's shared strings
(HTML for rich text), `Roo::Link` for hyperlinks, and error text such as
`"#DIV/0!"`. A row has one entry per column from A to its last cell, with
`nil` where there is no cell. The first argument is a path or an IO; `sheet:`
is a name or index (default: the first sheet); other keywords are
`Roo::Excelx.new` options. It closes the workbook when it is done, and
returns an Enumerator without a block. With the reader off, or on JRuby, it
reads through roo and yields the same rows.

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

- **roo 4.0.0's test suite.** `rake parity` runs roo's Excelx tests and specs
  with and without roo-calamine and requires the same results.
- **Every roo fixture.** `test/roo_fixtures_test.rb` compares all 84
  `.xlsx`/`.xlsm` files in roo's `test/files` cell by cell (class, value,
  type, formatted value, every instance variable) across open options,
  streaming options, and the `Roo::Base` API, including `parse` and `each`
  under nine option sets, with and without a block, and the single-cell reads,
  and whether `cells`, built after `each` and `parse`, holds the very values
  they returned.
- **Edge cases.** `test/edge_cases_test.rb` pins roo's answer for Integer vs
  Float, random numbers, dates, and times under every number and date style,
  number text only roo's constructors read, the 1904 date system, merged
  ranges, rich text, inline strings, shared formulas, error cells,
  namespaces, `parse` and `each` after `set` or with a block that changes
  the workbook, and more.
- **Values only.** Both suites also compare `Roo::Calamine.each_row_values`
  with the values of roo's `each_row_streaming(pad_cells: true)`, with the
  native reader on and off.

Known differences, by design:

- Code that overrides roo's private cell parsing (`SheetDoc#cell_from_xml`,
  `#extract_cells`, `Cell.cell_class`) is bypassed while the reader is on, and
  so are a `Roo::Excelx` subclass's `#cell` or `#each` inside `parse` and
  `each` with options, and redefinitions of the single-cell reads (see
  [How it works](#how-it-works)) in `Roo::Excelx` or `Roo::Excelx::Sheet`
  themselves (subclasses work). Under
  roo 4.0.0 so is an override of `initialize` on a `Cell` class or on
  `Roo::Excelx::Coordinate`.
- Building the whole `Sheet#cells` hash (`#to_csv`, `#set`, `#each` with
  options, …) after a read that does not need it (a single-cell read,
  `#last_row`, `#row`, `#each` without options, `#parse` without a block)
  costs more than building it from the start, because the sheet is parsed
  before any cell is built rather than alongside: on the 100k-row benchmark
  files, a single-cell read and then `#to_csv` take about 55% longer than
  `#to_csv` alone.
- On a sheet built whole (see [How it works](#how-it-works)), inside an
  `each` block without options, a write straight into `sheet_for(...).cells`
  (not through `set`) does not show in later rows.
- On XML that breaks partway, `each_row_streaming` raises the same
  `Nokogiri::XML::SyntaxError` as roo but yields a few more rows before it.
- Streaming uses about 15 MB more peak memory.

## How it works

roo-calamine prepends `Roo::Excelx::SheetDoc` and replaces the two places
where roo reads worksheet XML: `#cells` (behind `#cell`, `#row`, `#each`,
`#parse`, `#to_csv`, …) and `#each_row_streaming`. A Rust extension parses
the XML with [quick-xml](https://github.com/tafia/quick-xml) on a background
thread and builds the same `Roo::Excelx::Cell` objects. Under roo 4.0.0 it
sets each cell's instance variables the way roo's constructors do, without
running them; cells with a hyperlink, number text that is not plainly written,
and other roo versions go through roo's own constructors.

Single-cell reads (`#cell`, `#celltype`, `#formula`, `#excelx_type`,
`#excelx_value`, `#formatted_value`, `#excelx_format`, `#font`, `#empty?`),
`#column`, and `#first_row` … `#last_column` build only the cells they read:
the extension keeps the parsed sheet in compact native records and builds a
cell the first time it is read. `#row`, `#each` without options, and `#parse`
without a block read values straight from those records, and build a cell
only for a value that needs one (a hyperlink, a date with a time, a time). The
records keep each value they hand out, so a cell built later holds that very
object, as in roo. Anything that needs the whole `Sheet#cells` hash (`#each`
with options, `#parse` with a block, `#to_csv`, `#set`, …) builds it from the
records, keeping the cells and values already handed out. A sheet where some
cell would raise, or opened with `expand_merged_ranges`, is built whole, as
before.

Everything else stays roo's code: shared strings, styles, number formats,
the base date, hyperlinks, comments, images, and every open option. It works with
`Roo::Spreadsheet.open`, `Roo::Excelx.new`, and IO streams.

It also prepends `Roo::Excelx#parse` and `#each` for calls with options
(`headers`, `header_search`, header queries, `clean`). They keep roo's header
handling and build each row's Hash with less work than roo's one `#cell` call
per value. `parse` without a block builds each Hash from the records, or on a
sheet built whole groups the cells by row in one pass. `each`, and `parse`
with a block, look up each value in the `cells` hash, because the block may
change the workbook between rows.

`each` without options, behind `each_with_index`, `to_a`, and `parse` without
options, is roo's own loop over `#row`, so a block that calls `set` or
`reload`, or changes `default_sheet`, sees the change from the next row on. On
a sheet built whole of 3,000 rows or more, it reads the rows past the first
1,000 from an index built in one pass over the sheet's cells, and hands back
to roo's lookups once the block calls `set` or `reload`, or changes
`default_sheet`.

When a sheet uses XML the reader does not reproduce exactly the way Nokogiri
does (a non-UTF-8 encoding, a DTD, custom entities, or malformed XML), it
hands the sheet back to roo. A stream that hits such a spot continues in roo
from the next row.

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
