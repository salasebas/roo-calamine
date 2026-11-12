# Review ledger

Reviewer: Claude Fable 5.2, effort high. Approval covers the working experiment,
not adoption of the fork. The recommendation remains reject.

## Round 1

Verdict: changes. No findings declined.

- S1, should fix, confirmed. With either UTF-16 byte-order mark followed by
  UTF-8 XML, the candidate returned cell value 7 while roo returned nil.
  quick-xml's unified encoding feature consumed the mark. The Rust reader
  regression, public worksheet oracle, and shared-string oracle all failed
  before the fix. The first-event guard restores fallback before detection,
  retries interrupted input, and checks one-byte buffers. All three pass
  afterward. The fix adds 12 owned implementation lines, reducing the saving
  from 32 to 20.
- S2, should fix, confirmed. The lockfile contains 145 packages versus 40 in
  the baseline. The report now distinguishes the 46-package normal tree from
  the larger workspace/dev/optional graph gated by cargo-deny. The wider
  advisory/yank/license exposure is counted as a maintenance cost.
- S3, nice to have, confirmed and fixed. The old counter ignored a newly added
  candidate file. A temporary one-line adapter changed neither count before
  the fix; afterward it adds exactly one line. The candidate list now comes
  from the working tree independently of the baseline list.
- S4, nice to have, fixed. Usage now includes the optional CONTROL_LIB argument.

Round 1 scoped source/report changes were frozen while the reviewer ran.
Generated measurement files were excluded from that round and regenerated
against the updated baseline before filing.
