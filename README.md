# roo-calamine

Make [roo](https://github.com/roo-rb/roo) read `.xlsx` and `.xlsm` files
4–22× faster, with identical results and 4–14× less memory outside
streaming. Your roo code does not change.

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

## How it works

roo-calamine prepends `Roo::Excelx::SheetDoc` and replaces the two places
where roo reads worksheet XML: `#cells` (behind `#cell`, `#row`, `#each`,
`#parse`, `#to_csv`, …) and `#each_row_streaming`. A Rust extension parses
the XML with [quick-xml](https://github.com/tafia/quick-xml) on a background
thread and builds the same `Roo::Excelx::Cell` objects with roo's own
constructors.

Single-cell reads (`#cell`, `#celltype`, `#formula`, `#excelx_type`,
`#excelx_value`, `#formatted_value`, `#excelx_format`, `#font`, `#empty?`),
`#row`, `#column`, and `#first_row` … `#last_column` build only the cells
they read: the extension keeps the parsed sheet in compact native records and
builds a cell the first time it is read. Anything that needs the whole
`Sheet#cells` hash (`#each`, `#parse`, `#to_csv`, `#set`, …) builds the rest,
keeping the cells already handed out. A sheet where some cell would raise, or
opened with `expand_merged_ranges`, is built whole, as before.

Everything else stays roo's code: shared strings, styles, number formats,
dates, hyperlinks, comments, images, and every open option. It works with
`Roo::Spreadsheet.open`, `Roo::Excelx.new`, and IO streams.

It also prepends `Roo::Excelx#parse` and `#each` for calls with options
(`headers`, `header_search`, header queries, `clean`). They keep roo's header
handling and build each row's Hash from the `cells` hash. roo builds it with
one `#cell` call per value instead. `parse` without a block groups the cells
by row in one pass. `each`, and `parse` with a block, look up each value
directly, because the block may change the workbook between rows.

When a sheet uses XML the reader does not reproduce exactly the way Nokogiri
does (a non-UTF-8 encoding, a DTD, custom entities, or malformed XML), it
hands the sheet back to roo. A stream that hits such a spot continues in roo
from the next row.

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
  under nine option sets, with and without a block, and the single-cell reads.
- **Edge cases.** `test/edge_cases_test.rb` pins roo's answer for Integer vs
  Float, dates and the 1904 system, merged ranges, rich text, inline strings,
  shared formulas, error cells, namespaces, `parse` and `each` after `set`
  or with a block that changes the workbook, and more.
- **Values only.** Both suites also compare `Roo::Calamine.each_row_values`
  with the values of roo's `each_row_streaming(pad_cells: true)`, with the
  native reader on and off.

Known differences, by design:

- Code that overrides roo's private cell parsing (`SheetDoc#cell_from_xml`,
  `#extract_cells`, `Cell.cell_class`) is bypassed while the reader is on, and
  so are a `Roo::Excelx` subclass's `#cell` or `#each` inside `parse` and
  `each` with options, and redefinitions of the single-cell reads above in
  `Roo::Excelx` or `Roo::Excelx::Sheet` themselves (subclasses work).
- Reading the whole sheet after a single-cell read or `#last_row` costs about
  10–25% more than reading it whole from the start, because the sheet is parsed
  before any cell is built rather than alongside. `#each` and `#parse` build
  the whole sheet first, so they are not affected.
- On XML that breaks partway, `each_row_streaming` raises the same
  `Nokogiri::XML::SyntaxError` as roo but yields a few more rows before it.
- Streaming uses about 10 MB more peak memory.

## Benchmarks

Median of 3 runs on an Apple M5 Pro, Ruby 3.4.12. Memory is peak RSS.

| File | Scenario | roo | roo-calamine | Speedup |
|---|---|---:|---:|---:|
| 100k rows × 20 mixed columns | `each_row_streaming` | 14.51 s, 118 MB | 1.43 s, 132 MB | 10.2× |
| | `parse(headers: true)` | 23.34 s, 3423 MB | 2.91 s, 839 MB | 8.0× |
| | 10,000 random `cell(row, col)` | 20.28 s, 3272 MB | 0.91 s, 235 MB | 22.2× |
| | values only: `each_row_values` | 15.80 s, 118 MB | 0.81 s, 128 MB | 19.5× |
| 100k rows × 10 shared strings | `each_row_streaming` | 6.18 s, 136 MB | 0.46 s, 131 MB | 13.5× |
| | `parse(headers: true)` | 6.46 s, 1681 MB | 0.97 s, 396 MB | 6.7× |
| | 10,000 random `cell(row, col)` | 6.12 s, 1598 MB | 0.47 s, 160 MB | 13.2× |
| | values only: `each_row_values` | 5.91 s, 120 MB | 0.38 s, 165 MB | 15.5× |

For the values-only rows, plain roo runs the `each_row_streaming(pad_cells:
true)` code above.

```sh
cd bench && bundle install && bundle exec ruby run.rb
```

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
