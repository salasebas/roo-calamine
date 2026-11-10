# roo-calamine

Make [roo](https://github.com/roo-rb/roo) read `.xlsx` and `.xlsm` files
4–12× faster, with identical results and 4–5× less memory for whole-sheet
reads. Your roo code does not change.

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

Everything else stays roo's code: shared strings, styles, number formats,
dates, hyperlinks, comments, images, and every open option. It works with
`Roo::Spreadsheet.open`, `Roo::Excelx.new`, and IO streams.

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
  streaming options, and the `Roo::Base` API.
- **Edge cases.** `test/edge_cases_test.rb` pins roo's answer for Integer vs
  Float, dates and the 1904 system, merged ranges, rich text, inline strings,
  shared formulas, error cells, namespaces, and more.
- **Values only.** Both suites also compare `Roo::Calamine.each_row_values`
  with the values of roo's `each_row_streaming(pad_cells: true)`, with the
  native reader on and off.

Known differences, by design:

- Code that overrides roo's private cell parsing (`SheetDoc#cell_from_xml`,
  `#extract_cells`, `Cell.cell_class`) is bypassed while the reader is on.
- On XML that breaks partway, `each_row_streaming` raises the same
  `Nokogiri::XML::SyntaxError` as roo but yields a few more rows before it.
- Streaming uses about 10 MB more peak memory.

## Benchmarks

Median of 3 runs on an Apple M5 Pro, Ruby 3.4.9. Memory is peak RSS.

| File | Scenario | roo | roo-calamine | Speedup |
|---|---|---:|---:|---:|
| 100k rows × 20 mixed columns | `each_row_streaming` | 15.46 s, 122 MB | 1.37 s, 133 MB | 11.3× |
| | `parse(headers: true)` | 24.42 s, 3295 MB | 3.87 s, 861 MB | 6.3× |
| | 10,000 random `cell(row, col)` | 19.86 s, 3276 MB | 2.16 s, 673 MB | 9.2× |
| | values only: `each_row_values` | 16.46 s, 119 MB | 0.78 s, 143 MB | 21.0× |
| 100k rows × 10 shared strings | `each_row_streaming` | 5.65 s, 123 MB | 0.46 s, 131 MB | 12.4× |
| | `parse(headers: true)` | 6.49 s, 1680 MB | 1.45 s, 390 MB | 4.5× |
| | 10,000 random `cell(row, col)` | 5.35 s, 1592 MB | 0.70 s, 306 MB | 7.6× |
| | values only: `each_row_values` | 6.02 s, 124 MB | 0.39 s, 166 MB | 15.6× |

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
