# Calamine shared-reader experiment

**Decision: retain main's current backend. Reject adoption of this candidate.**
The fork works end to end and shares Calamine's existing XLSX parser. It adds
78 owned production lines and slows several public reads, including mixed-100k
`cell` (open plus 10,000 random reads) by about 3.7×. The upstream merge was small. Merge
conflicts are not the reason for this recommendation.

This is a second implementation for [#75](https://github.com/salasebas/roo-calamine/issues/75).
[PR #76](https://github.com/salasebas/roo-calamine/pull/76) moved our reader into
a new Calamine module. That did not test whether using Calamine's existing
reader would reduce maintenance. This branch tests that design directly;
it does not change or close the first PR.

## Shared reading path

```text
Roo API -> existing native store/adapter -> XlsxCellStream::for_roo(BufRead)
                                           |
stock XlsxCellReader::next_cell ------------+
stock value/style/formula APIs -------------+
                                           v
                              next_cell_formula_record_impl
                                 read_value / read_v
                                 read_formula
                                 read_string_with_bufs
                                           |
                              raw row text and attributes
                                           v
                              existing Roo conversion/store
```

The fork generalizes the existing `XlsxCellReader` over `BufRead` and keeps
its ZIP-based public name as a type alias. Stock Calamine and Roo enter the
same existing cell loop. The old `reader.rs`, attribute submodule and test
file are removed from the extension; there is no selectable second backend.

The direct constructor takes no workbook metadata. It does not call
`Xlsx::new`, reparse Roo's styles/relationships/shared strings, resolve shared
indices to plain text, or construct a dense `Range`. Sparse records, Ruby
object identity, mutations, version gates, bounded batches, interruption,
read-ahead, early close and the workbook ZIP snapshot remain the existing
implementation. The two entity decoders use Calamine's existing helper.

Raw mode preserves `<v>1</v>` versus `<v>1.0</v>` and absent versus empty values.
It retains `r`, `s`, `t`, formula text/order and shared-string indices for Roo.
Stock Calamine keeps its typed-value path. Roo's row selection, inline text,
first-value rule and XML/Nokogiri checks remain custom compatibility code.
It takes more than an Integer/Float switch to preserve this contract.

The reused API and baseline are Calamine's
[0.38 streaming reader](https://github.com/tafia/calamine/blob/a4dba3b5309d81b2774018a13be39e68d938c3c7/src/xlsx/cells_reader.rs)
and [inline-string helper](https://github.com/tafia/calamine/blob/a4dba3b5309d81b2774018a13be39e68d938c3c7/src/xlsx/mod.rs).
The vendored source is pinned in [UPSTREAM](../vendor/calamine/UPSTREAM).

## Owned code and maintenance

Physical production lines include comments and blanks. Production-file test
modules, test files, benches, scripts and docs are excluded. In existing
upstream files, added/replacement production lines count as ours; unchanged
lines do not. Deleted upstream lines are reported separately, not credited
as deletions of our old implementation. The full delta, including deletions,
still needs review at every update.

The counts include cosmetic changes in the shared helpers: local variable
names, match-arm spelling and comments, plus equivalent rich-string buffer
rewrites. The measured snapshot retains this churn. A cleanup could reduce
the patch surface; its exact line saving and runtime effects were not measured.

| Term | Lines |
|---|---:|
| Main at `194b149d64a2682fa43fc54319a640dfe4eb300e` | 6,783 |
| Remaining extension and Ruby implementation | 5,627 |
| Custom new fork files | 942 |
| Added/replacement lines in existing upstream files | 292 |
| Candidate owned total | 6,861 |
| Net saving | **−78 (1.15% more)** |
| Upstream production lines deleted by the patch | 246 |
| No-fork control owned total | 6,733 (50 saved) |

[Per-file counts](calamine-shared-reader-evidence/counts.json) and the
[complete patch](calamine-shared-reader-evidence/fork-0.38.patch) make the
boundary inspectable. Reproduce with:

```sh
ruby bench/fork_counts.rb /path/to/clean-calamine-0.38
ruby script/calamine_patch.rb /path/to/clean-calamine-0.38 > roo.patch
```

The root implementation shrinks by 1,156 lines. The 1,141-line worksheet
reader/attribute checker disappears from the extension, but its Roo rules
mostly remain in the fork. The fork adds 199 attribute-check lines, 284 XML
wrapper/check lines, 275 model/helper lines and 184 row-selection/batching
lines. The shared cell loop and value/formula/string helpers carry the other
292 added/replacement production lines. This replaces the parser loop rather
than simply relocating it, but does not reduce total owned code.

Existing upstream functions changed include the constructor, `next_cell`,
`next_cell_with_style_id`, both formula-record entry points, their shared
loop, `read_value`, `read_v`, `read_formula`, `read_formula_record`,
`next_formula`, `read_string` and `read_string_with_bufs`. Raw-mode branches
retain first-value selection, nested text/CDATA, empty values, XML validation
and row boundaries. Stock methods now share the formula-record loop; their
value-only formula-order behavior has a separate regression test.

New state is explicit: `XlsxCellStream.roo: Option<RooState>` and a raw flag
in the value context. `RooState` holds row selection, the row under assembly,
reused capacities, EOF and a deferred error. `XmlReader` holds an optional
`Checks` state for the existing namespace, XML-space and syntax rules.
Stock readers have no compatibility state. No new application cache, backend
registry or broad fallback was introduced. The generic stream still carries
Calamine's empty metadata/formula buffers for Roo; the per-value `String`
copy into a row is extra work relative to main's fused plain-cell shortcut.

The extension keeps archive, rich shared-string HTML, sheet-part extraction,
conversion, sparse storage and Ruby integration. They implement Roo behavior
rather than generic tokenization. Calamine's default shared-string result
loses indices/HTML; its metadata APIs expose different semantics and would
repeat Roo's work. Replacing them would require more adapters or more fork
changes, not established deletions.

The [no-fork control](calamine-shared-reader-evidence/no-fork.patch) from #76
shares entity decoding and validates shared strings in one pass. Its 6,733
lines were recounted here, and its 17 public digests and timings were rerun.
It is an experimental alternative, not a second implementation in this
candidate. A [ZIP control](calamine-shared-reader-evidence/zip-control.md)
found rubyzip accepts bytes for a wrong CRC where `zip` reports an error.
That result is retained from #76; no ZIP saving is credited here.

Ruby tests/support remain 3,343 -> 3,355 physical lines. The same 475-line
reader test file becomes 485 lines after constructor/import changes and the
UTF-16 regression. Additional test harness declarations and the new stock
formula-order test are excluded from production counts. Original oracle
expectations were not relaxed. This report, evidence, bench tools, patch
script, packaging checks and CI integration are additional maintenance
outside the production-line metric.

## Runtime, memory and distribution

[Measurements](calamine-shared-reader-evidence/timings.md) contain all 17
public scenarios, variation, peak RSS, control measurements, warm queries,
component phases, sparse probes and the Calx array reference. Raw samples
and fixture hashes are alongside them. Baseline, candidate and control were measured together
in fresh processes on the same Ruby 3.4.12 / Apple M5 Pro machine; every
public scenario's digest matches plain Roo 4.0.0. No samples were removed.

The shared engine lacks main's fused plain-cell fast path and creates
intermediate strings before appending to the raw row. It also reads cell
attributes twice: Roo's `cell_attributes` builds the raw record, then
Calamine's `get_attrs!` reads `r`, `s` and `t` although raw mode does not use
those second results. The cold-reading regression is consistent with these
differences; no profiler establishes how much each contributes.
Transplanting the old fast parser into the fork
would add more owned code and has not been credited as an improvement.
Warm Ruby dispatch and cell construction remain after either parser finishes.
Calx has different values/APIs and is a reference, not a compatible target.

The sparse A1/XFD1048576 probe retains a 242-byte Ruby store wrapper and
materializes two cells, with dimensions 1,048,576 × 16,384. Wrapper size is
not total native memory. Existing large late-read memory tests also pass.

The normal dependency graph grows from 17 to 46 unique packages. The root
lockfile grows from 40 to 77, rather than #76's 145: `vendor/calamine` is
explicitly excluded from the workspace, so its dev-dependencies do not enter
the gem's lock/deny graph. The temporary upstream CI checkout is excluded
separately so Cargo treats it as an independent crate. Cargo's
[workspace rules](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-members-and-exclude-fields)
explain automatic path membership. CI explicitly formats and lints the fork,
then applies the exact patch to a pinned upstream checkout and runs its tests
and doctests with the upstream fixtures.

Calamine enables quick-xml's `encoding` feature and adds zip's `zlib-rs`
backend through Cargo feature unification. The first-byte FF/FE guard keeps
UTF-16-marked input on Roo's fallback path; worksheet and shared-string
regressions cover it. The backend change is part of the measured candidate;
it is not an isolated parser-only benchmark. Cargo-deny retains registry/git
source restrictions and allows the Zlib license only for `zlib-rs`.

Fresh target-directory release builds took 12.61 s for main and 15.52 s for
the candidate; binaries were 1,032,560 and 1,183,136 bytes. These are single
local builds with registry/toolchain caches warm, not compile-time forecasts.
[Commands and sizes](calamine-shared-reader-evidence/builds.json).
The source gem includes the pinned fork/license and requires Rust 1.88.
A local isolated install served the smoke reads natively. The PR matrix
provides precompiled gems, Linux/macOS/Windows Ruby versions and inert JRuby
validation; its final result is recorded in the PR.

## Upstream update exercise

The starting fork was ported onto actual upstream 0.37.0
`f2c5948242fdfe390e292313a1f87f9db19b9633`, then upstream 0.38.0
`a4dba3b5309d81b2774018a13be39e68d938c3c7` was merged into it.
Both versions use the existing shared core, not the independent reader from
#76. The [0.37 patch](calamine-shared-reader-evidence/fork-0.37.patch) and
[exercise record](calamine-shared-reader-evidence/upgrade.json) retain the
starting delta and revisions.

There were two conflict hunks: module imports in `src/lib.rs` and
`src/xlsx/mod.rs`. The cell engine merged automatically. Manual integration
also routed upstream's newly added `next_cell_with_style_id` through the
shared core and initialized its new styles field in the direct constructor.
Those changes matter even though Git did not flag them as conflicts.

The merge session spans 74 seconds between Git commits, including interactive
waits and one discarded resolution-script retry. It excludes initial 0.37
porting and later compilation/tests. It is not a clean estimate of future
human maintenance effort. This example shows a feasible small update; it
does not prove all future updates will be as small or expensive.

The 0.37 checkout passed 232 tests (one skipped) and 66 doctests. The updated
checkout passed 318 tests (one skipped) and 67 doctests, including 15 reused
Roo reader tests and one new stock-only regression. That last value-reset
fix/test was added after the merge exercise. All Rust sources in the final
validated checkout match the candidate byte for byte. The Roo suite and
six parity suites also pass on the candidate. No upstream test expectation
was edited.

## Maintaining this fork if adopted later

1. Start from the pinned upstream revision in `vendor/calamine/UPSTREAM` and
   generate the patch above. Keep that commit and upstream history in a fork
   branch, or retain a versioned patch against each upstream release.
2. Merge a chosen upstream release. Review both conflict hunks and clean
   merges in cell iteration, raw text, inline strings, attributes and errors.
   Confirm every new stock entry point uses the shared core when applicable.
3. Run Calamine's unchanged fixtures/tests/doctests plus the Roo reader tests,
   native tests, the full Roo oracle suite and `rake parity`. Reproduce new
   compatibility gaps against plain Roo before fixing them.
4. Update the vendored snapshot, `UPSTREAM`, patch-script revision, CI fixture
   revision, dependency pin/lock and Rust requirement together. Keep the
   upstream manifest, examples and license intact. Do not ship test fixtures.
5. Recount both new files and edits to existing files. Recheck paired runtime,
   memory/sparse/late reads, source install and the platform-gem matrix.

A fork can inherit fixes in the shared XML/cell/string helpers. Roo-only
semantics and validation still belong to us; default Calamine numeric/date
and workbook changes do not automatically benefit the raw path. On this
implementation, the measured benefit does not offset the added code,
dependencies and runtime regression. Keep main's faster working backend;
retain this complete branch as evidence. This does not rule out a future
upstream raw API or a smaller integration with a measured benefit.
