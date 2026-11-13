# roo-turbo

Make [roo](https://github.com/roo-rb/roo) read `.xlsx` and `.xlsm` files
**16–36× faster** on large sheets, with **2–31× less memory** and identical
results. Your roo code does not change.

```ruby
# Gemfile
gem "roo"
gem "roo-turbo"
```

```ruby
xlsx = Roo::Spreadsheet.open("orders.xlsx") # or Roo::Excelx.new("orders.xlsx")
xlsx.each_row_streaming { |row| ... }        # read by the native reader
xlsx.parse(headers: true)                    # this too
```

> **Unofficial.** roo-turbo is not affiliated with or endorsed by roo-rb or
> the roo maintainers.

## Install

Add the gem next to roo, as above. Bundler requires it on `Bundler.require`;
without Bundler, `require "roo/turbo"`.

### Migrating from roo-calamine

Replace `gem "roo-calamine"` with `gem "roo-turbo"`; keep `gem "roo"`.
Use `require "roo/turbo"` and `Roo::Turbo` in new code. The old
`require "roo/calamine"`, version require and `Roo::Calamine` constant remain
available as compatibility aliases in the new gem. Load one plugin gem at a
time; loading the old plugin before roo-turbo raises a migration error.

`ROO_CALAMINE` still works when `ROO_TURBO` is unset. When both are set,
`ROO_TURBO` takes precedence.

| | |
|---|---|
| Ruby | CRuby 3.3, 3.4, 4.0 |
| roo | `~> 4.0` |
| Precompiled | Linux x86_64/aarch64 (glibc, musl), macOS x86_64/arm64, Windows x64 |
| Other platforms | the source gem compiles with Rust 1.86 or later |
| JRuby | installs without Rust and stays inert: roo reads every file itself |

## Benchmarks

100,000 rows × 20 mixed columns, Apple M5 Pro, Ruby 3.4.12. Time and peak
memory include opening the workbook.

| | roo 4.0.0 | roo + roo-turbo | Faster |
|---|---:|---:|---:|
| `each_row_streaming` | 13.49 s · 119 MB | **0.59 s · 51 MB** | 23× |
| `parse(headers: true)` | 19.61 s · 3300 MB | **0.75 s · 282 MB** | 26× |
| `each` | 18.81 s · 3282 MB | **0.62 s · 174 MB** | 30× |
| 10,000 random `cell(row, col)` | 17.25 s · 3272 MB | **0.48 s · 104 MB** | 36× |
| `to_csv` | 21.72 s · 3369 MB | **0.77 s · 118 MB** | 28× |
| `to_yaml` | 24.26 s · 3430 MB | **1.28 s · 322 MB** | 19× |
| `to_xml` | 50.81 s · 3991 MB | **1.04 s · 711 MB** | 49× |
| `to_matrix` | 19.97 s · 3287 MB | **0.65 s · 216 MB** | 31× |
| [`each_row_values`](#values-only) | 13.43 s · 118 MB | **0.44 s · 47 MB** | 31× |

On the other test files it is 18–43× faster with shared strings, 16–33× with
unique strings, and 2.6–10× on a 100-row sheet. See
[bench/README.md](bench/README.md) for every file, the method, and how to run
it. [bench/RESULTS.md](bench/RESULTS.md) times every public roo read. It
ties roo only where roo reads the file itself: CSV, and XML roo-turbo
hands back.

## Values only

If you only need values, `Roo::Turbo.each_row_values` skips roo's cell
objects. It is roo-turbo's own API, not roo's, and using it changes nothing
in what roo does.

```ruby
Roo::Turbo.each_row_values("orders.xlsx", sheet: "Orders") do |values|
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
Roo::Turbo.enabled = false # from the next sheet read on
```

```sh
ROO_TURBO=0 bundle exec rails server # also: false, off
```

Either one makes roo read exactly as it does without the gem.
`Roo::Turbo.enabled?` tells whether the next read uses the native reader,
and `Roo::Turbo.available?` whether the extension is loaded.

## How it works

roo-turbo prepends modules to roo's classes. A read such as
`each_row_streaming` reaches roo-turbo first, which either does the work in
a Rust extension and returns the objects roo would return, or hands the call
back to roo.

- **Opening:** worksheets stay in the zip until they are read.
- **Parsing:** worksheet XML is streamed with
  [quick-xml](https://github.com/tafia/quick-xml) instead of loaded into a
  Nokogiri document.
- **Lazy cells:** the sheet lives in compact native records, and a
  `Roo::Excelx::Cell` is built only when something reads it.
- **Output:** `to_csv`, `to_yaml`, `to_xml`, and `to_matrix` are written from
  those records.
- **Fallback:** XML the reader cannot reproduce exactly the way Nokogiri does
  goes back to roo.

Styles, number formats, comments, images, and every open option stay roo's
code. roo is the test oracle: roo 4.0.0's own suite, every roo fixture, and the
edge-case suites must give the same results with and without roo-turbo.
[docs/how-it-works.md](docs/how-it-works.md) has the details and the test
coverage.

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
  - redefinitions of the
    [single-cell reads](docs/how-it-works.md#cells-and-values) in `Roo::Excelx`
    or `Roo::Excelx::Sheet` themselves (subclasses work);
  - under roo 4.0.0, an override of `initialize` on a `Cell` class or on
    `Roo::Excelx::Coordinate`;
  - under roo 4.0.0, `Sheet#pad_cells` redefined in `Roo::Excelx::Sheet`
    itself (subclasses and prepended overrides work).

</details>

<details>
<summary>Behavior and performance</summary>

- **Building `Sheet#cells` late costs more.** Building the whole hash (`#set`,
  for example) after a read that does not need it (a single-cell read,
  `#last_row`, `#row`, `#each`, `#parse`) costs more than building it from the
  start, because the sheet is parsed before any cell is built rather than
  alongside.
- **Non-UTF-8 text set by hand.** A cell value set to non-ASCII text in an
  encoding other than UTF-8 can make `to_csv` raise
  `Encoding::CompatibilityError`, where roo writes it, when its line also
  holds non-ASCII UTF-8 text. Cells read from a file are always UTF-8.
- **Writes straight into `cells`.** On a
  [sheet built whole](docs/how-it-works.md#cells-and-values), inside an `each`
  block without options, a write straight into `sheet_for(...).cells` (not
  through `set`) does not show in later rows.
- **Broken XML.** On XML that breaks partway, `each_row_streaming` raises the
  same `Nokogiri::XML::SyntaxError` as roo but yields a few more rows before
  it.
- **Errors come when a sheet is read.** A workbook leaves its worksheets in
  the zip, so a worksheet whose compressed data is broken, or larger than the
  zip says, raises roo's error when it is read rather than in
  `Roo::Excelx.new`. The paths in `#sheet_files` exist only once roo's own
  code has read them.

</details>

## Development

```sh
bundle install
bundle exec rake compile   # build the extension
bundle exec rake test      # unit, fixture, and edge-case tests
bundle exec rake parity    # roo 4.0.0's own suites, with and without roo-turbo
bundle exec rubocop
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

## License

MIT. See [LICENSE](LICENSE).
