# Unique-string costs

Issue #50 compares the generated `unique-strings-100k.xlsx`: 100,001 rows,
10 columns, and 1,000,010 distinct shared strings. The base is `2cd40eb`,
including #60. Measurements use release builds, Ruby 3.4.12, roo 4.0.0,
calx 0.1.0, an Apple M5 Pro, and Darwin 25.6.0.

## End-to-end measurements

Each entry is the median of five fresh processes. Before, after, and calx
runs alternate on the same fixture. Library loading is outside the timer;
opening the workbook is inside it. RSS is the process peak in MiB.

| Scenario | Before, seconds / MiB | After, seconds / MiB | Less time | calx, seconds / MiB | After / calx time |
|---|---:|---:|---:|---:|---:|
| Streaming | 0.459 / 251.1 | 0.474 / 251.0 | unchanged | 0.366 / 152.3 | 1.29 |
| `parse(headers: true)` | 0.881 / 251.5 | 0.592 / 211.2 | 32.8% | 0.497 / 206.0 | 1.19 |
| `each` | 0.821 / 206.1 | 0.473 / 171.2 | 42.4% | 0.342 / 152.1 | 1.38 |
| 10,000 random `cell` reads | 0.612 / 207.4 | 0.451 / 157.8 | 26.3% | 0.365 / 206.3 | 1.23 |
| `to_csv` | 0.636 / 225.6 | 0.479 / 175.5 | 24.8% | 0.362 / 153.2 | 1.32 |
| Values only | 0.437 / 160.8 | 0.394 / 160.9 | unchanged | 0.368 / 152.1 | 1.07 |

The implementation changes document reads only. Timing differences in streaming
and values-only runs are measurement variation, not improvements from this patch.

## Selection cache

The old document converter holds a Ruby Hash entry per shared-string index.
`ObjectSpace.memsize_of` measures about 32 MiB for one million entries.
Every shared cell probes it while validating records, finding bounds, and
returning values. In one instrumented `each` run, bounds take 0.038 s and
row emission takes 0.164 s before the change, versus 0.008 s and 0.052 s
with the Array cache. Instrumented times are diagnostic, not table medians.

When HTML is disabled, or its deferred table covers the plain table and has
no tags roo selects, the converter captures selections in a shallow duplicate
of the plain Array. Ruby can share its backing storage until an entry changes.
It checks the HTML table's length because appending to the public plain Array
does not extend HTML: roo must still raise for an absent HTML entry.
Rich text, publicly materialized HTML, out-of-range indices, and spellings
Ruby must interpret retain the existing Hash. No per-sheet Array is allocated
for rich text. Checking a deferred HTML table scans it once per converter;
the copied Array shares storage until it or the public table changes.
Streaming still chooses again after each block call.

The copy preserves object identity and in-place string edits, while replacing
an entry of either public table cannot replace a document's earlier selection.
The existing index-spelling test now covers plain text as well as rich text.
Removing the shallow copy makes its plain-text case fail with `new plain`
instead of `edited`. One regression test covers an appended plain entry with
no matching HTML entry; without the length check it returns `extra` instead
of roo's `NoMethodError`. Both tests were observed failing before their fixes.

The implementation follows Magnus's existing
[`RArray::dup` and `entry` APIs](https://docs.rs/magnus/0.9.3/magnus/struct.RArray.html)
and this extension's marked Ruby caches. No dependency is added.

## String floor and allocations

`bench/unique_strings.rb floor` measures 40,000,400 bytes for the strings and
8,000,120 bytes for their Array: 45.78 MiB in total. Every value fits in a
40-byte embedded Ruby String. The strings alone cost 38.15 MiB.
This is a floor for retaining the complete table, not a minimum RSS for the
process or a claim that 10,000 lookups need a million Ruby objects.

| Scenario | Peak above the complete-table floor, MiB | Ruby objects allocated, approximately |
|---|---:|---:|
| Streaming | 205.3 | 3.30 million |
| `parse(headers: true)` | 165.4 | 1.20 million |
| `each` | 125.4 | 1.10 million |
| 10,000 random `cell` reads | 112.0 | 1.04 million |
| `to_csv` | 129.8 | 1.00 million |
| Values only | 115.1 | 1.10 million |

The allocation counts come from the profiling command, separate from timed
runs. The cache changes storage and lookup cost, not the number of values roo
returns. The RSS differences also include Ruby, libraries, native buffers,
temporary objects, and allocator capacity.

## Remaining work performed for roo

`parse` and values only are within 20% of calx. The other reference scenarios omit work
performed by roo's API. The main remaining document cost is loading and
validating roo's cell records, not the selection cache or CSV formatting.

The profiling command measures the following seconds. `Store.read` includes
the nested shared-table call. The last column subtracts that call once.

| Scenario | `Store.read` | Shared tables | Worksheet records and validation |
|---|---:|---:|---:|
| `parse` | 0.400 | 0.191 | 0.209 |
| `each` | 0.400 | 0.189 | 0.211 |
| Random `cell` | 0.396 | 0.183 | 0.213 |
| `to_csv` | 0.393 | 0.185 | 0.208 |

That 0.21 s phase parses worksheet XML, retains raw cell fields, and validates
that every roo cell can be constructed before yielding document results.
Roo raises for the entire sheet if constructing any cell raises, including
one after the requested cell or row. `Store.read` therefore checks each
record through `Converter::builds_safely`; it cannot stop after the first row
or the 10,000 requested cells. It retains coordinates, style, formula, and
content so later roo metadata lookups and cell construction still work after
the first read. Calx's value Arrays do not provide that roo cell model.
The residual excess of 0.013–0.062 s is in this measured document-loading
work, which is shared by `each`, `parse`, and CSV.
These phase timings identify the current cost of that contract, not a claim
that its parser or representation cannot be optimized further.

- Streaming constructs one million `Cell::String` objects and one million
  coordinates, in addition to values and row Arrays. Values only omits those
  objects and takes 0.394 s versus streaming's 0.474 s, a 0.080 s difference.
  The excess over 1.2 times the calx reference is 0.034 s.
- Document reads retain records needed for `celltype`, style, formula,
  coordinates, later cell construction, and mutable values. A million records
  occupy 19.07 MiB, their sorted index 3.81 MiB, plus text and formula storage.
  Profiling separates shared-table creation, record loading, bounds, and row
  emission. Shared-table creation takes about 0.19 s and allocates one million
  strings. Record loading includes that time; it must not be added again.
- `each` also follows changes to the current sheet or materialized cells after
  every block. `bench/each.rb` measures loaded-store row emission at 0.049 s
  for `each` versus 0.033 s for direct native iteration. Bounds take another
  0.008 s. Calx's streaming reference retains neither roo's records nor those
  checks. The remaining end-to-end excess over the 20% target is 0.062 s.
  The same reader's document `each` costs 0.079 s more than values only,
  which does not retain the sheet's cell model for later reads.
- `parse` and its calx reference both allocate a header Hash per row. Roo
  retains them as its return value; calx retains value Arrays and discards the
  temporary Hashes. Allocation counts alone do not establish a time penalty
  for those Hashes. The measured parse time is now within 20% of calx.
- Random `cell` reads allocate roughly 40,000 objects beyond loading the
  table, including up to 10,000 roo cells and their coordinates. They also
  resolve and validate the sheet on every call. Calx indexes value Arrays.
  Roo's excess over the target is 0.013 s; its peak is now 48.5 MiB below calx.
  With loading and random-coordinate generation outside the timer,
  `bench/open.rb` measures 12.3 ms for the first 10,000 reads, 4.85 ms for
  cached reads, and 2.44 ms for cached reads directly from the native store.
- `to_csv` returns one 15,588,971-byte String, quotes each string field, checks
  escaping and roo types, and writes line endings. The calx reference only
  joins each row with commas and discards the result. Its output has no
  quotes, line endings, or retained CSV String. Roo's excess over the target
  is 0.044 s; the returned String alone is 14.87 MiB. CSV is only 0.006 s
  slower than document `each` end to end, so the remaining time gap is
  attributed to document loading, not to quoting alone.

## Leads not taken

Deferred plain strings could reduce sparse random-cell memory further. That
requires preserving document selections when public arrays are replaced,
string identity, and reads after close. This patch retains eager plain strings
and measures their cost rather than introducing another deferred table.
The 10,000-value string floor alone is about 0.38 MiB, so sparse reads still
have avoidable memory above the full-table floor.

Packing records from 20 to 16 bytes would save 3.81 MiB per million records,
smaller than the measured cache saving, and requires encoding the existing
row, style, kind, content, and formula information differently. It is unchanged.
Accounting for native row-buffer capacities in Ruby's size callback did not
reduce streaming peak RSS in a local experiment, so that change was removed.
Streaming's transient cell and native-row lifetimes remain a memory lead.

## Reproduce

Build with `bundle exec rake compile`. Generate fixtures with
`cd bench && bundle exec ruby generate.rb`. From `bench`, run:

```sh
bundle exec ruby scenario.rb roo-calamine each fixtures/unique-strings-100k.xlsx
bundle exec ruby scenario.rb calx each fixtures/unique-strings-100k.xlsx
bundle exec ruby unique_strings.rb profile each
bundle exec ruby unique_strings.rb floor
bundle exec ruby each.rb each fixtures/unique-strings-100k.xlsx
bundle exec ruby each.rb unchecked fixtures/unique-strings-100k.xlsx
```

Repeat timed commands in fresh processes and take the median. Replace `each`
with `stream`, `parse`, `cell`, `csv`, or `values` for the other scenarios.
For correctness, append `digest` to `scenario.rb` and compare roo-calamine with
plain `roo`. Run `bundle exec rake parity` from the repository root as well.
