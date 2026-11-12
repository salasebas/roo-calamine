# How roo-calamine works

roo-calamine keeps roo in place and reroutes the reads that dominate its run
time. When it loads, it prepends modules to roo's classes (`Module#prepend`):
a call such as `sheet.each_row_streaming` reaches roo-calamine's method
first. That method either does the work in the Rust extension and returns the
objects roo would return, or calls `super` and lets roo do it.

Everything else stays roo's code: styles, number formats, the base date,
comments, images, and every open option.

## Where roo spends its time

| roo 4.0.0 | roo-calamine |
|---|---|
| Extracts every worksheet of the zip to a temporary directory when it opens a workbook | Keeps the worksheets in the zip and inflates one only when it is read |
| Loads each sheet into a Nokogiri DOM and walks it with XPath | Streams the XML with [quick-xml](https://github.com/tafia/quick-xml) |
| Builds a `Roo::Excelx::Cell` for every cell, even to read one | Keeps the sheet in compact native records and builds a cell the first time it is read |
| Writes `to_csv`, `to_yaml`, `to_xml`, and `to_matrix` slot by slot through `#cell` | Writes them from the native records |

## Ruby and Rust

The extension is a native library, like Nokogiri's, written in Rust instead of
C. `rake compile` (or the precompiled gem) produces `roo_calamine.bundle` or
`.so`. Requiring it runs the function marked `#[magnus::init]`, which defines
`Roo::Calamine::Native` and its classes. [magnus](https://github.com/matsadler/magnus)
converts values between the two languages through Ruby's C API.

While the extension parses, it releases Ruby's global VM lock
(`rb_thread_call_without_gvl`), so other Ruby threads keep running.
Large streaming reads also inflate and parse on background threads while Ruby
consumes the rows that are ready.

## Building cells

Under roo 4.0.0 the extension sets each cell's instance variables the way
roo's constructors do, without running them. Some cells still go through roo's
own constructors:
- cells with a hyperlink;
- number text that is not plainly written;
- every cell under other roo versions.

## Reading cells

- **Single-cell reads** (`#cell`, `#celltype`, `#formula`, `#excelx_type`,
  `#excelx_value`, `#formatted_value`, `#excelx_format`, `#font`, `#empty?`)
  and `#first_row` … `#last_column` build only the cells they read, the first
  time each is read.
- **`#row`, `#column`, `#each`, and `#parse`** read values straight from the
  records. They build a cell only for a value that needs one: a hyperlink, a
  date with a time, or a time.
- **The records keep each value they hand out**, so a cell built later holds
  that very object, as in roo.
- **Anything that needs the whole `Sheet#cells` hash** (`#set`, `clean: true`,
  …) builds it from the records, keeping the cells and values already handed
  out.
- **A sheet built whole:** a sheet where some cell would raise, or one opened
  with `expand_merged_ranges`, is built whole, as in roo.

roo finds a sheet's hyperlinks, and its merged ranges under
`expand_merged_ranges`, with XPath on a Nokogiri document of the whole sheet.
The extension reads those `hyperlink` and `mergeCell` elements alone, and
copies of roo's loops turn them into the same hyperlinks and merged cells.

## `each` and `parse`

- **With options** (`headers`, `header_search`, header queries, `clean`):
  - roo-calamine's `#parse` and `#each` keep roo's header handling, and build
    each row's Hash without roo's one `#cell` call per value.
  - `parse` without a block builds each Hash from the records. On a sheet built
    whole, it groups the cells by row in one pass.
  - `each`, and `parse` with a block, resolve the current sheet on each row and
    read its records until its `cells` hash is built.
  - `clean: true` builds the cells before iterating, as in roo.
- **Without options**, `each` (behind `each_with_index`, `to_a`, and `parse`
  without options) walks the records in order. On a sheet built whole of
  3,000 rows or more, it answers the rows past the first 1,000 from an index
  built in one pass over the sheet's cells.

Either way, a block that calls `set` or `reload`, or changes `default_sheet`,
sees the change from the next row on, as in roo's `Base#each`.

## Whole-sheet output

Under roo 4.0.0 roo-calamine replaces the writers behind `#to_csv`,
`#to_yaml`, `#to_xml`, and `#to_matrix`. roo writes every slot from A1 to the
last row and column through `#empty?`, `#cell`, and `#celltype`, which look
the sheet and the cell up again for each slot.

- **While the cells are in the native records**, the extension writes the
  output from them in row and column order, without building the `cells` hash:
  - it formats strings and booleans itself;
  - it hands numbers and dates to the same Ruby formatting roo uses;
  - it builds a cell only for a field that needs one;
  - it asks the cells and values already handed out, which a caller may have
    changed.
- **Once the `cells` hash is built**, roo writes YAML, XML, and Matrix itself.
  roo-calamine still writes the CSV: it walks the hash once in row and column
  order, and sorts the keys first when the hash is out of order.
- **Under other roo versions**, roo writes all four.

## Shared strings

roo-calamine prepends `Roo::Excelx::SharedStrings#to_a` and `#to_html`. The
extension reads `sharedStrings.xml` and builds the requested array the way
roo does, `_x000D_` fix and rich-text HTML included.

- **HTML is built only when needed:** it stays in a compact native table until
  a cell needs rich text or something calls `#to_html`. `disable_html_wrapper`
  skips that table until something asks for it.
- **Public arrays** hold the same mutable strings the reader uses.
- **Reading `#to_html` before `#to_a`** reads the file again for the plain
  array.
- **Back to roo:** if the file uses XML that Nokogiri reads differently (see
  [Fallback to roo](#fallback-to-roo)), or a `vertAlign` without `val` (roo
  raises on it), roo builds the arrays itself.

## Opening a workbook

roo extracts every worksheet of a workbook to a temporary directory when it
opens it. With the reader on, roo-calamine takes a snapshot of the workbook,
whether it is opened from a path or an IO:
- ZIPs up to 256 KiB stay in memory;
- larger ZIPs are copied to roo's temporary directory, which roo still creates
  for its extractors and fallbacks;
- an IO is read once, from its beginning.

- **Worksheets stay in the snapshot.**
  - The reader inflates the one it reads.
  - Streaming entries under 256 KiB inflated are parsed on demand, without a
    worker thread.
  - A worksheet is extracted only when roo's own code reads it (`#dimensions`,
    or XML the reader hands back to roo).
- **The parts roo still extracts** (`sharedStrings.xml`, styles, comments,
  relationships, images) are inflated by the extension, not rubyzip. That keeps
  a workbook with many unique strings from growing by several times its
  `sharedStrings.xml`.
- **An entry the extension cannot extract**, such as an encrypted one or one
  that inflates past its size, is left to rubyzip, which raises or warns as in
  roo.
- **As with roo**, a workbook reads the same after its file is changed or
  deleted.

## Fallback to roo

When a sheet uses XML the reader does not reproduce exactly the way Nokogiri
does (a non-UTF-8 encoding, a DTD, custom entities, or malformed XML), it hands
the sheet back to roo. A stream that hits such a spot continues in roo from the
next row. Shared strings follow the same rules, and also go back to roo when
the part holds a processing instruction or a `<t>` holds markup other than text
and CDATA.

## How it is tested

roo is the oracle: every check takes roo's own output as the expected result.

| Suite | What it compares with roo |
|---|---|
| `rake parity` | roo 4.0.0's own Excelx tests and specs, run with and without roo-calamine |
| `test/roo_fixtures_test.rb` | All 84 `.xlsx`/`.xlsm` files in roo's `test/files`. For each cell it compares the class, value, type, formatted value, and every instance variable. It covers open options, streaming options, `parse` and `each` under nine option sets with and without a block, and the single-cell reads. |
| `test/edge_cases_test.rb` | Edge cases, listed below |
| `test/shared_strings_test.rb` | `SharedStrings#to_a` and `#to_html`, read in either order |
| `test/to_csv_test.rb` | The bytes `to_csv` returns and writes |

The edge-case suite covers:
- Integer vs Float, dates, and times under every number and date style;
- number text only roo's constructors read;
- the 1904 date system;
- merged ranges, rich text, inline strings, shared formulas, and error cells;
- `parse` and `each` after `set`, or with a block that changes the workbook.

The CSV suite covers:
- every cell type, separator, sheet, and file argument;
- cells out of order, repeated, or merged;
- values changed in place before the call;
- the file roo leaves behind when a cell raises.

Both the fixture and the edge-case suites also compare
`Roo::Calamine.each_row_values` with the values of roo's
`each_row_streaming(pad_cells: true)`, with the native reader on and off.
