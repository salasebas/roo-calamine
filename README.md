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
| 100k rows × 20 mixed columns | `each_row_streaming` | 14.26 s, 122 MB | 1.31 s, 135 MB | 10.9× |
| | `parse(headers: true)` | 20.96 s, 3431 MB | 3.48 s, 860 MB | 6.0× |
| | 10,000 random `cell(row, col)` | 20.74 s, 3267 MB | 1.94 s, 660 MB | 10.7× |
| 100k rows × 10 shared strings | `each_row_streaming` | 5.25 s, 124 MB | 0.43 s, 133 MB | 12.3× |
| | `parse(headers: true)` | 6.27 s, 1663 MB | 1.47 s, 396 MB | 4.3× |
| | 10,000 random `cell(row, col)` | 5.40 s, 1598 MB | 0.70 s, 307 MB | 7.8× |

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
