# roo-calamine

Add one line to your Gemfile and [roo](https://github.com/roo-rb/roo) reads
`.xlsx` and `.xlsm` files 6–8× faster, with less memory and the same results.
Your roo code does not change.

```ruby
# Gemfile
gem "roo"
gem "roo-calamine"
```

```ruby
xlsx = Roo::Spreadsheet.open("orders.xlsx") # or Roo::Excelx.new("orders.xlsx")
xlsx.each_row_streaming { |row| ... }        # now read by the native reader
xlsx.parse(headers: true)                    # this too
```

> **Unofficial.** roo-calamine is not affiliated with or endorsed by roo-rb or
> the roo maintainers. It depends on roo `~> 4.0` and changes only how roo
> reads worksheet XML.

## Contents

- [Install](#install)
- [What it accelerates](#what-it-accelerates)
- [What stays roo's own code](#what-stays-roos-own-code)
- [Turning it off](#turning-it-off)
- [Platforms and JRuby](#platforms-and-jruby)
- [Correctness: roo is the oracle](#correctness-roo-is-the-oracle)
- [Benchmarks](#benchmarks)
- [Design](#design)
- [Development](#development)
- [Releasing](#releasing)
- [Credits](#credits)

## Install

```ruby
gem "roo-calamine"
```

Bundler requires `roo/calamine` on `Bundler.require`, and that turns the
native reader on. Without Bundler, `require "roo/calamine"`.

Precompiled gems exist for x86_64 and aarch64 Linux (glibc and musl), x86_64
and arm64 macOS, and 64-bit Windows (`x64-mingw-ucrt`), for Ruby 3.3, 3.4,
and 4.0. On those platforms `gem install` needs no Rust toolchain. Anywhere
else the source gem compiles the extension, which needs Rust 1.86 or later.

## What it accelerates

roo reads every worksheet in one of two places, and roo-calamine replaces
both, through `Module#prepend` on `Roo::Excelx::SheetDoc`:

| roo method | What it now does |
|---|---|
| `SheetDoc#cells`, used by `#cell`, `#row`, `#column`, `#each`, `#parse`, `#first_row` … `#last_column`, `#formula`, `#formulas`, `#celltype`, `#excelx_type`, `#excelx_value`, `#empty?`, `#to_s`, `#to_csv` … | reads the sheet in Rust and builds the same `Roo::Excelx::Cell` objects with roo's own constructors |
| `SheetDoc#each_row_streaming` and `#each_cell`, used by `#each_row_streaming` with `pad_cells`, `offset`, and `max_rows` | streams rows from Rust; a row becomes cells only when roo asks for them |

Because the hook sits under `Roo::Excelx`, it covers `Roo::Spreadsheet.open`,
`Roo::Excelx.new` called directly, and streams (`Roo::Excelx.new(io)`), with
every roo option (`expand_merged_ranges`, `empty_cell`, `no_hyperlinks`,
`disable_html_wrapper`, `only_visible_sheets`, `cell_max`, …).

## What stays roo's own code

The native reader returns roo's own cell objects, so everything that works
on a cell keeps working through roo's Ruby code: `formatted_value`,
`excelx_format`, `font`, `hyperlink`, `comment`, `label`, `images`, styles,
shared strings (including the HTML that roo builds for rich text), base
dates, and the number-format rules that decide between Integer, Float, Date,
DateTime and Time.

The reader hands a sheet back to roo, which reads it with Nokogiri as usual,
when the XML uses something it does not reproduce exactly the way Nokogiri
does:

- an XML encoding other than UTF-8 (some Windows Phone exports are UTF-16);
- a document type declaration, whose entities and attribute defaults Nokogiri
  applies;
- an entity reference other than the five XML ones and character references;
- XML that is not well-formed: characters XML does not allow, unbalanced or
  missing tags, malformed attributes and names, `]]>` in text, `--` in
  comments, anything but whitespace, comments, and processing instructions
  around the root element, and a late XML declaration.

For `each_row_streaming`, the rows read before such a spot are kept and roo
continues from the next row, so nothing is read twice and the rows come out
in order.

## Turning it off

```ruby
Roo::Calamine.enabled = false # this process, from the next sheet read on
```

```sh
ROO_CALAMINE=0 bundle exec rails server # also: false, off
```

Both are read on every sheet read, so they work at runtime. With either one,
roo reads exactly as it does without the gem. `Roo::Calamine.enabled?` tells
whether the next read uses the native reader, and `Roo::Calamine.available?`
whether the extension is loaded at all.

## Platforms and JRuby

| | |
|---|---|
| Ruby | CRuby 3.3, 3.4, 4.0 (roo's own floor is 3.3) |
| roo | `~> 4.0` |
| OS | Linux (glibc, musl), macOS, Windows |
| JRuby 10 | installs and stays inert: roo reads every file itself |

A native extension cannot load on JRuby. There the gem's `extconf.rb` writes
an empty Makefile, so `gem install` and `bundle install` succeed without Rust,
`require "roo/calamine"` loads nothing native, `Roo::Calamine.available?` and
`enabled?` are `false`, and `Roo::Excelx` is not patched. CI proves this on
JRuby 10 (`test/jruby_smoke.rb`).

## Correctness: roo is the oracle

The goal is behavior identical to roo. Every check below takes roo's own
output as the expected result.

**roo 4.0.0's own test suite.** `rake parity` checks out the roo `v4.0.0` tag
(pinned by SHA), runs its Excelx tests and specs once with plain roo and once
with roo-calamine loaded, and requires the same results:

| Suite | Tests | roo: failed / skipped | roo-calamine: failed / skipped | Native reads | roo cell parses (plugin run) | Same |
|---|---:|---:|---:|---:|---:|---|
| `test/roo/test_excelx.rb` | 25 | 0 / 1 | 0 / 1 | 19 | 14 | yes |
| `test/test_roo.rb` | 41 | 0 / 1 | 0 / 1 | 66 | 0 | yes |
| `test/excelx` | 51 | 0 / 0 | 0 / 0 | 0 | 0 | yes |
| `spec/lib/roo/excelx_spec.rb` | 65 | 0 / 0 | 0 / 0 | 35 | 0 | yes |
| `spec/lib/roo/strict_spec.rb` | 8 | 0 / 0 | 0 / 0 | 12 | 0 | yes |
| `spec/lib/roo/excelx` | 62 | 0 / 0 | 0 / 0 | 1 | 0 | yes |

The skip in both runs is roo's own (`LONG_RUN`). The 14 cells roo parses in
the plugin run belong to `Pfand_from_windows_phone.xlsx`, whose worksheets
are UTF-16 and go to roo on purpose.

**Every roo fixture, every read path.** `test/roo_fixtures_test.rb` opens
each of the 84 `.xlsx`/`.xlsm` files in roo's `test/files` and compares plain
roo with roo-calamine on: the full `cells` hash (every cell's class, value,
type, formatted value, and every instance variable) under five sets of open
options; `each_row_streaming` under five sets of `pad_cells`/`offset`/
`max_rows`; and the `Roo::Base` API (bounds, rows, columns, formulas,
`to_csv`). It also asserts that the native reader, not roo's parser, served
each read. 252 tests, 0 differences.

**Edge cases.** `test/edge_cases_test.rb` builds workbooks for the cases a
reader most easily gets wrong, and pins roo's own answer for each first:
Integer vs Float from the written text (`1` vs `1.0`, `1E3`, a 20-digit
integer), Date vs DateTime vs Time, the 1904 date system, 1-based and
missing coordinates, padded and empty cells, merged ranges, hyperlinks,
shared strings with rich text and `_x000D_`, inline strings, formulas
(normal, shared, and after the value), cached values, error cells, booleans,
CDATA and entities, prefixed namespaces, errors roo raises, and a stream
that hands over to roo halfway.

### Known differences

No difference in what roo returns has been found. Differences that remain by
design:

- Code that overrides roo's private cell parsing, such as
  `Roo::Excelx::SheetDoc#cell_from_xml` or `#extract_cells`, or
  `Roo::Excelx::Cell.cell_class`, is bypassed while the native reader is on.
  Public methods and the `Cell` classes themselves are used as they are.
- On a worksheet whose XML breaks partway, `each_row_streaming` raises the
  same `Nokogiri::XML::SyntaxError` as roo, but yields every row before the
  bad spot; roo stops a little earlier, where libxml2's read-ahead reaches
  it (241 of 250 good rows in `test/edge_cases_test.rb`).
- Resource use changes (see [Benchmarks](#benchmarks)): peak memory is 3–5×
  lower for `cells`-based reads and a little higher for streaming, and each
  sheet read starts a short-lived native thread that parses ahead of Ruby.

## Benchmarks

BENCHMARK_TABLE

`bench/run.rb` generates the workbooks with
[caxlsx](https://github.com/caxlsx/caxlsx): 100,000 rows × 20 columns of
mixed types, 100,000 rows × 10 columns of repeated shared strings, and 100
rows × 20 columns. Each cell of the table is the median of three runs, each
in a fresh process; memory is the process's peak RSS. Before timing, one run
of roo and one of roo-calamine hash every value they read, and the two
hashes must match. [calx](https://github.com/7a6163/calx) is a reference for
a reader that skips roo's cell model: it returns plain values, not roo
cells, so it is not a drop-in replacement.

```sh
cd bench && bundle install && bundle exec ruby run.rb
```

## Design

### Why a plugin

roo is maintained and widely used, so a fork would split users. A pull request
to roo would make every roo user build a Rust extension. A plugin keeps roo
as it is and maintains only the fast path; anything the fast path does not
handle stays roo's own code.

### The seam

Three places were candidates: (a) a `Roo::Excelx` subclass registered in
`Roo::CLASS_FOR_EXTENSION`, (b) replacing how the cells of a sheet are
built, and (c) a `prepend` on roo's classes. roo-calamine uses (b) through
(c): `Roo::Excelx::SheetDoc` is prepended with `#cells`,
`#each_row_streaming`, and `#each_cell`.

All three run the same native code, so they cost the same per cell; what
differs is what they cover. On the 100k × 20 workbook, `Sheet#cells` takes
2.2 s through both `Roo::Spreadsheet.open` and `Roo::Excelx.new`. (a) would
miss `Roo::Excelx.new` and still have to replace sheet reading inside the
subclass. Prepending `SheetDoc` covers every entry point, keeps roo's own
`Sheet#each_row` (with its `pad_cells`, `offset`, and `max_rows` logic), and
lets the kill switch call `super`.

### The engine: calamine's parser stack, not calamine's cell model

The extension was planned over [calamine](https://github.com/tafia/calamine)
0.38. calamine's cell reader turns every number into an `f64` and returns
shared strings already resolved, but roo's results depend on the raw XML:

- roo returns `1` (Integer) for `<v>1</v>` and `1.0` (Float) for `<v>1.0</v>`,
  and exposes the raw text as `cell_value`/`excelx_value`. caxlsx, Apache POI
  and others write `1.0` for floats, so calamine would turn those Float
  columns into Integer. calamine declined to keep integers apart
  ([tafia/calamine#669](https://github.com/tafia/calamine/pull/669): "all
  numbers are f64").
- roo wraps rich-text shared strings in HTML by their index; calamine returns
  plain text.
- roo's handling of `t="e"`, `t="inlineStr"` with a `<v>`, phonetic text, and
  rows without `r` differs from calamine's.

Measured on roo, XML parsing was not the cost anyway: on 400,000 cells,
Nokogiri's parse took 0.23 s of roo's 2.8 s; the rest was per-cell Ruby work.
So the extension reads the worksheet with
[quick-xml](https://github.com/tafia/quick-xml), the XML parser calamine is
built on, into raw cell records that mirror what roo's `cell_from_xml` sees,
and builds cells with roo's own constructors. The name stays; calamine
itself is not a dependency.

### Optimizations

Each was measured, and adopted only with a win and unchanged results.

| | Measured | Decision |
|---|---|---|
| **1. Streaming, not whole-sheet loading** | Both read paths stream the XML; nothing loads a whole sheet into Rust. `each_row_streaming(max_rows: 100)` on the 100k file returns in 0.08 s. Peak RSS for streaming is in the table. | Adopted |
| **2. Parsing without the GVL** | A native thread parses the next batch of about 16,384 cells while Ruby turns the current one into cells; Ruby releases the GVL only when it has to wait for a batch. 100k × 20, alone / four threads reading at once / next to a busy Ruby thread, median of three: `cells` 1.58 / 6.96 / 4.07 s, streaming 1.18 / 4.70 / 3.93 s. Holding the GVL while parsing, as calx does: 2.49 / 11.65 / 6.75 s and 2.29 / 8.61 / 5.23 s. Releasing it around each batch instead: 2.52 / 10.15 / 5.24 s and 2.63 / 5.53 / 5.82 s, and next to a busy thread with 128-row batches 19.8 s, because each handoff waits for the other thread's time slice. (Alternatives: single runs.) | Adopted, as a read-ahead thread |
| **3. One String per shared string** | Cells reuse the String objects of roo's own shared-string table, as roo does. 1M shared-string cells: 0.73 s and 2.0M allocated objects, against 0.80 s and 3.0M with a new String per cell (roo: 5.46 s, 14.6M). Inline strings and numbers keep one String per cell, because roo returns distinct, mutable strings for them. | Adopted |
| **4. No conversions roo does not need** | No Nokogiri nodes, attribute strings, or `"A1"` strings per cell; a style's format and type are looked up once per style; a streamed row becomes cells only when roo yields it: `each_row_streaming(offset: 90_000)` takes 0.67 s against 1.82 s for every row. Lazy Date objects inside `cells` were rejected: roo's cells compute their value in the constructor, and the oracle tests compare every instance variable. | Adopted, except lazy values |

A fifth change came from profiling: keeping a row's text in one buffer and
its attributes as typed values cut the Rust parse of the 100k × 20 sheet from
0.79 s to 0.47 s. The well-formedness checks added later cost some of that
back, on the read-ahead thread, where it does not show in wall time
(streaming the same sheet: 1.41–1.45 s before and after).

## Development

```sh
bundle install
bundle exec rake compile   # build the extension into lib/roo/calamine/
bundle exec rake test      # unit, fixture, and edge-case tests (fetches roo 4.0.0's fixtures)
bundle exec rake parity    # roo 4.0.0's own suites, with and without roo-calamine
bundle exec rubocop
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

## Releasing

Releases are cut by [release-please](https://github.com/googleapis/release-please)
from Conventional Commit titles (pull requests are squash-merged), and
published by `.github/workflows/release.yml` through RubyGems.org
[trusted publishing](https://guides.rubygems.org/trusted-publishing/): no API
key is stored anywhere.

1. Merging a pull request into `main` updates release-please's release pull
   request (version and CHANGELOG). `release.yml` runs the required checks
   on it, because pull requests opened with `GITHUB_TOKEN` trigger no
   workflows by themselves.
2. Merging the release pull request tags `vX.Y.Z`, creates the GitHub
   release, builds the source gem and the seven platform gems, installs each
   on its platform without Rust, and pushes all eight to RubyGems.org from the
   `rubygems` environment.

The trusted publisher on RubyGems.org (registered as a pending publisher
before the first release, since the gem did not exist yet):

| Field | Value |
|---|---|
| Gem name | `roo-calamine` |
| Repository owner | `salasebas` |
| Repository name | `roo-calamine` |
| Workflow filename | `release.yml` |
| Environment | `rubygems` |

## Credits

- [roo](https://github.com/roo-rb/roo) (MIT) does all the spreadsheet work;
  this gem only reads worksheet XML for it.
- [calamine](https://github.com/tafia/calamine) (MIT) and its XML parser
  [quick-xml](https://github.com/tafia/quick-xml) (MIT).
- [calx](https://github.com/7a6163/calx) (MIT), the reference for the native
  extension; `.github/workflows/build-gems.yml` adapts its cross-compilation
  workflow.
- [magnus](https://github.com/matsadler/magnus) and
  [rb-sys](https://github.com/oxidize-rb/rb-sys) for the Ruby bindings and
  the platform gems.

MIT License. See [LICENSE](LICENSE).
