# Calamine backend evaluation

Reject this candidate for adoption. It runs the complete plugin through a
Calamine fork, but saves only 32 owned implementation lines out of 6,645
(0.48%). A tested control without Calamine saves 50 lines. Keep this PR as
the working experimental candidate; do not merge it as a backend upgrade.
This decision applies to the implementation measured here, not every possible
Calamine integration.

## Candidate and baseline

The actual baseline is roo-calamine
`43914e90cc018ba07fcc29f92d3e2a866ab3eccb`, rather than the later research
revision cited in issue #75. The machine runs Ruby 3.4.12 and Cargo 1.99.1
on arm64 macOS, Apple M5 Pro. Both native extensions use the same release profile, thin
LTO, one codegen unit, Ruby, fixtures, and Ruby adapters.

The fork starts at Calamine 0.38.0,
`a4dba3b5309d81b2774018a13be39e68d938c3c7`. Its upstream source, MIT license,
manifest, examples, and changelog are in `ext/roo_calamine/calamine`.
`UPSTREAM` identifies the exact source and owned changes. Cargo uses an exact
version and a local path. The source gem carries the fork, so installation
fetches registry dependencies rather than a mutable Git branch. The source
policy still denies unknown Git and registry sources. The new `zip` dependency
activates `zlib-rs`; only that crate receives the additional Zlib license allowance.
The normal dependency tree grows from 17 to 46 packages, including the
extension itself in both counts (`cargo tree -p roo_calamine --edges normal
--prefix none --no-dedupe`, unique package lines).
The minimum Rust version rises from 1.86 to 1.88, as required by upstream.

The official implementation used as the starting point is
[Calamine 0.38.0](https://github.com/tafia/calamine/tree/a4dba3b5309d81b2774018a13be39e68d938c3c7).
Its `src/utils.rs::unescape_entity_to_buffer` supplies entity decoding.
The new `calamine::roo::SheetReader` is an additive raw-record entry point
for any `BufRead`. This is an intentional public generic input, needed to
accept the existing ZIP read-ahead channel and file buffers without copying.
It preserves raw numeric text, shared indices, empty rows, formula order,
roo coordinate rules, and the Nokogiri compatibility checks.

```text
roo public API
  existing Ruby modules and native conversion/store
    calamine::roo::SheetReader
      quick-xml and Calamine entity decoding
```

`Xlsx::new`, `DataRef`, and dense `Range` storage are not on this path.
Opening does not parse styles, relationships, or shared strings again in
Calamine. The existing sparse store, GVL release, interrupt handling,
read-ahead workers, snapshots, early close, and fallback to roo remain in use.
There is one native worksheet backend on this branch.

## What moves and what disappears

Count physical production lines, including comments and blanks. Cut Rust files
at `#[cfg(test)]`; exclude `tests.rs`, benchmarks, and documentation. Count
only added or edited upstream lines, not unchanged Calamine source.
`ruby bench/fork_counts.rb` regenerates [the counts](calamine-fork-evidence/counts.json).

| Term | Lines |
|---|---:|
| Baseline implementation | 6,645 |
| Old implementation removed | 1,161 |
| Replacement imports, calls, module export, and separator added | 5 |
| Remaining roo-calamine implementation | 5,489 |
| New fork implementation, including relocated rules | 1,122 |
| Edited upstream lines | 2 |
| Total owned candidate implementation | 6,613 |
| Net owned saving | 32 |

The 1,156-line net reduction in this repository is mostly relocation, not removal.
The five replacement lines are the module re-export, two imports, the
shared-string entity call, and the separator before the test harness.
The worksheet record model, row/cell interpretation, attribute validation,
Nokogiri checks, and optimized plain-cell path move into `src/roo`.
The old module is removed. Ruby integration re-exports the fork's module
through its existing `reader` interface. No runtime backend selector, cache,
or fallback is added. The fork adds no worksheet state beyond the existing
reader state. Upstream's entity helper replaces two owned decoders.

The fork changes two existing upstream lines: a public module declaration
and the visibility of the existing entity helper. It adds three files:
the raw reader, attribute checker, and the relocated 475-line reader test
file. The fork owns 43 functions, mostly transferred from the old reader.
All 3,271 lines of Ruby tests and support remain unchanged. Upstream cell interpretation and its tests keep their original
semantics. No oracle expectations or compatibility promises change.
The new test harness makes the relocated reader tests run in the ordinary
extension test command. Test code is not counted as a production saving.

Conversion, sparse storage, Ruby integration, and exports remain owned.
Shared-string parsing still needs roo's HTML, whitespace, mutation, and
`_x000D_` behavior. The upstream plain-string result cannot replace it.
Sheet parts still need raw references and namespace behavior rather than
Calamine's parsed ranges. Archive handling still has to honor rubyzip's
selected entry, errors, and snapshot. Rewriting those rules inside Calamine
would add more owned fork code; it would not make the rules disappear.

## Control without a fork

The control starts at the same baseline. It shares entity decoding between
worksheet and shared-string readers using quick-xml's existing
`resolve_xml_entity`, and sends shared-string events through the existing
validated event reader. That removes the second validation pass and its
scoped worker. It changes no Ruby interface and saves 50 implementation
lines, leaving 6,595. Its shared-string suite passes 21 tests and 165
assertions; its Rust suite passes all 25 tests.

The control patch is retained with the evidence. It is an experimental
control, not a second backend in the product. Sheet-part parsing already
uses validated events in the actual baseline, so there is no second
sheet-part validation worker left to consolidate.

A selective ZIP control tested both a valid stored entry and an entry whose
local and central CRCs were changed to the same wrong value. rubyzip returned
`"hello"` for both. `zip` 9.5.1 returned `Invalid checksum` for the damaged
entry. The native archive implementation follows rubyzip here. We therefore
claim zero verified archive deletion from replacing it with `zip`. This
probe is an incompatibility example, not a complete archive compatibility audit.

## Runtime evidence

`bench/fork_comparison.rb` uses fresh processes, rotates baseline/candidate
order, checks digests against plain roo before timing, and records every
sample. With an optional fourth argument it also measures the no-fork
control. Fixtures are shared byte for byte. The timer includes open/load
and public Ruby dispatch; these numbers are not parser microbenchmarks.

The measured cases cover open, random `cell` and `celltype`, full and limited
streaming, row iteration, CSV, shared strings, and unique strings.
`bench/fork_probe.rb` also measures open plus first read and two passes over
identical coordinates generated before timing. Sparse reads use only A1 and
XFD1048576, including late cell materialization. Both implementations retain
242 bytes in the sparse store and materialize exactly two cells; the bounds
are 1,048,576 rows and 16,384 columns. No dense bounding-box allocation occurs.

[Timing tables and raw samples](calamine-fork-evidence/timings.md) contain
all 255 measurements and 17 oracle hashes. Small differences in these
runs do not establish a speed benefit from the fork. The parsing and Ruby
conversion algorithms are substantially the same. Other tasks also ran on
the host during the experiment; variation must be read with that limitation.

The built extension grows from 1,030,576 to 1,087,520 bytes (5.5%).
A single release build in a fresh target directory takes 12.07 seconds for
the baseline and 14.01 seconds for the candidate. Both use the same compiler
and shared compiler cache. These single builds are distribution evidence,
not a stable build-time benchmark. [Commands and elapsed times](calamine-fork-evidence/builds.json)
record the exact flags.

## Compatibility and distribution

Both baseline and candidate pass `bundle exec rake test`:
717 runs, 18,496 assertions, zero failures, errors, or skips.
Both pass `bundle exec rake parity`: the six upstream suites have identical
results with and without the plugin, including the two existing skips.
Native-reader counters confirm the candidate uses the native path.

`cargo nextest run` passes all 25 extension/reader tests.
`cargo test --doc`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --all --check`, RuboCop, and cargo-deny pass.
The fork's upstream tests are also exercised from a full upstream checkout,
including its original fixtures; see the update exercise below.

The source gem is built and checked with `script/check_gem.rb`, installed
into an isolated gem directory, and read through `script/smoke.rb`.
The smoke confirms native document and streaming reads. The source gem
includes the fork's source, pin, and license. No source-policy exception
for Git is needed. Linux/macOS/Windows, Ruby 3.3/3.4/4.0, precompiled gems,
musl, and JRuby are validated by the existing PR matrix; their live results
must pass before the PR is considered ready. Local proof covers arm64 macOS
on CRuby 3.4.12, not the other platforms.

## Real upstream update

The same fork was built first on 0.37.0
(`f2c5948242fdfe390e292313a1f87f9db19b9633`) and merged forward to 0.38.0
(`a4dba3b5309d81b2774018a13be39e68d938c3c7`). This is a real upstream
change to `xlsx/cells_reader.rs`, `utils.rs`, and the module declarations,
not an empty update.

Applying the 0.38 patch backwards to 0.37 required one conflict resolution
in `src/lib.rs`. Merging the resulting fork forward also produced one
conflict hunk in that file. One edit kept `mod oleps`, `pub mod roo`, and
`mod style` together. The entity-helper visibility change merged cleanly.
A timed replay measured 7.583 seconds from starting the merge through
agent inspection, the one resolution, and its commit. The subsequent full
upstream tests and doctests took 16.153 seconds with warm build caches,
for 23.736 seconds total. These are agent execution times on this host,
not an estimate of a human maintainer's review time.

The 0.37 fork passes 232 tests with one existing skip, plus 66 doctests.
The upgraded 0.38 fork passes 317 tests with one existing skip, plus 67
doctests. No expected result changes were needed. Its `src` delta matches
the vendored candidate. The small conflict cost is evidence in the fork's
favor; release frequency alone does not establish expensive maintenance.

## Update and validate the fork

Keep a full upstream checkout with its history and fixtures. Check out the
pinned upstream revision, then generate the current patch:

```sh
git clone https://github.com/tafia/calamine.git /tmp/calamine
git -C /tmp/calamine checkout --detach a4dba3b5309d81b2774018a13be39e68d938c3c7
ruby script/calamine_patch.rb /tmp/calamine > /tmp/roo.patch
git -C /tmp/calamine switch -c roo-update
git -C /tmp/calamine apply /tmp/roo.patch
git -C /tmp/calamine add src
git -C /tmp/calamine commit -m 'Add roo-compatible raw streaming records'
```

Merge the chosen upstream release into that branch. Inspect and resolve
each conflict. Run its full fixture suite and doctests:

```sh
cargo nextest run --manifest-path /tmp/calamine/Cargo.toml
cargo test --manifest-path /tmp/calamine/Cargo.toml --doc
```

Copy its manifest, license, docs, examples, benches, and `src` back into the
vendored directory. Do not copy `.git`, build output, or its test fixtures
into the gem. Update `UPSTREAM`, the exact Cargo version, the patch script's
revision, lockfile, and source-count assumptions together. Then repeat the
extension checks, roo oracle/parity, benchmark digests, source installation,
and PR platform builds. Review the fork delta against the new upstream,
especially the two edits to existing upstream code.

## Decision

The complete additive reader works and preserves the current plugin contract.
It adds upstream releases, dependencies, a higher compiler minimum, vendored
source, and an update procedure to save 32 implementation lines. The no-fork
control removes more owned code with fewer dependencies. There is no measured
benefit here that justifies adopting this fork. Keep the existing backend and
consider the control separately. Broader changes to Calamine remain a separate
experiment; this result does not prove their possible savings.
