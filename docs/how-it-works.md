# How roo-calamine works

roo-calamine keeps roo's API and replaces its busiest read paths with a Rust
extension. It returns roo's objects and follows roo's value rules. Unsupported
files go through roo's own reader.

## Where the time goes

| roo 4.0.0 | roo-calamine |
|---|---|
| Extracts every worksheet when opening a workbook | Leaves worksheets in a ZIP snapshot until needed |
| Loads worksheet XML into Nokogiri and walks it with XPath | Streams raw rows through the Calamine fork's XLSX cell reader |
| Builds every cell before reading the sheet | Keeps compact records and builds cells when requested |
| Writes whole-sheet output through repeated cell lookups | Visits the native records in order |

Styles, formats, comments, images, and open options keep roo's behavior.

This experiment configures Calamine's existing cell stream over a `BufRead`.
It shares the cell loop, value/formula readers and inline-string helper with
Calamine's stock APIs. It does not construct `Xlsx` or `Range`, load workbook
metadata a second time, or allocate a rectangle for distant coordinates.
The [evaluation](calamine-shared-reader.md) explains the remaining Roo rules
and the recommendation to retain main's current backend.

## Ruby and Rust

The extension uses [magnus](https://github.com/matsadler/magnus) and Ruby's C API
to build the same classes roo builds. Parsing releases Ruby's VM lock. Large
streams inflate and parse ahead while Ruby consumes the available rows.

Under roo 4.0.0, known cell layouts are filled directly. Non-text links and
unusual number text still use roo's constructors. Other roo versions keep
their constructors throughout.

## Cells and values

`cell` reads immediate values and shared strings from the records; other
values stay in their roo cells. `celltype` reads known types from the records.
Single-cell metadata reads build only the cells they request. Rows, columns,
and iteration read values from the records. Inline and `t="str"` links become
`Roo::Link` values without a full cell; shared-string links still build one.

A value already returned stays the same object when its full cell is built.
Requesting `Sheet#cells` or writing with `set` builds the whole cell hash.
Sheets with merged-range expansion, or a cell that would
raise, are also built whole.

Link targets and single-cell references are read natively. Metadata uses the
same XML checks as the cell reader. Range links and merged cells follow roo's
rules.

## Iteration

`each` without options walks the native records in order. Once full cells
exist, large sheets use a row index. A block that writes, reloads, or changes
the active sheet sees that change on the next row, as in roo.

Header queries and `parse` retain roo's header handling. Each row hash comes
from the native records. `clean: true` applies roo's sanitizer once per string
and keeps cleaned values separate from raw cell values. Later reads and cell
materialization preserve roo's `value` and `cell_value` behavior. Sheets
already built whole use roo's cleaning loop.

`each_row_streaming` returns roo cells and preserves its padding, offsets,
and row limits. `Roo::Calamine.each_row_values` returns the same values as a
padded stream, without the cell wrappers.

## Whole-sheet output

CSV, YAML, XML, and Matrix output visit records in row and column order. The
writers reuse values and cells already returned to the caller, including
in-place edits. Numbers and dates keep roo's formatting.

Once full cells exist, roo writes YAML, XML, and Matrix. The extension still
writes CSV, sorting cell coordinates when needed. If a field raises, CSV
leaves the same partial file as roo. Other roo versions use roo's writers.

## Shared strings

Plain strings and rich-text HTML follow roo's decoding rules, including the
`_x000D_` fix. HTML stays in a compact native table until requested. Public
arrays contain the same mutable strings used by the reader.

Reading HTML before plain text reads the part again for plain strings.
Unsupported XML and malformed rich text go back to roo.

## Opening a workbook

Opening takes a snapshot, so later edits or deletion of the original file do
not change the workbook being read.

- ZIPs up to 256 KiB stay in memory; larger ZIPs use roo's temporary directory.
- Worksheets stay compressed until read. Small streaming sheets parse on
  demand; larger ones use background workers.
- The extension inflates metadata and shared strings instead of rubyzip.
  Entries it cannot extract go through rubyzip and keep roo's errors.

## Fallback

Non-UTF-8 encodings, DTDs, custom entities, and malformed XML go through roo
when the native reader cannot reproduce Nokogiri's result. A stream resumes
in roo after the rows already yielded. Shared strings use the same checks.

## Verification

Tests compare results with plain roo, including object identity and errors.

| Check | Coverage |
|---|---|
| `rake parity` | roo's Excelx tests and specs with and without the extension |
| `roo_fixtures_test.rb` | roo's workbook fixtures, cell state, options, and public reads |
| `edge_cases_test.rb` | Types, formats, XML recovery, merges, and edits during iteration |
| `cell_construction_test.rb` | Lazy values, links, and identity after cell construction |
| `shared_strings_test.rb` | Plain text, HTML, and mutable arrays |
| `to_csv_test.rb` | Output bytes, edits, separators, and partial-file errors |

Fixture and edge-case tests also compare value-only reads with padded roo
streams, with the native reader enabled and disabled.
