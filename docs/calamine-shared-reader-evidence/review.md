# Independent review

The original reviewer, Claude Fable 5.2 high, failed before a verdict because
its weekly quota was exhausted. It supplied no findings or approval. The user
then selected Claude Opus 5.5 high.

## Round 1 — Opus 5.5 high

Verdict: CHANGES. Runtime source reviewed at `3e96b60` (behavior source
`a1a680b`). The reviewer confirmed the shared cell engine, stock and Roo
semantics, encoding fallback, unchanged expectations and measurement counts.

| Id | Severity | Outcome |
|---|---|---|
| S1 | Must fix | Fixed: restore runner-provided `grep -E`, keeping the `vendor/` filter. Exact workflow step reproduced `exit=0, gems=false` without `rg`; fixed step writes `gems=true`. Vendor-only changes match; docs-only changes do not. |
| S2 | Must fix | Fixed: exclude `tmp/calamine-upstream` from the workspace. Reproduced Cargo's workspace error before fixing, then ran the exact pinned-checkout/patch/test command in that layout successfully. |
| S3 | Nice to have | Documented: counts include helper renames, comments, match spelling and buffer-expression churn. Cleanup deferred from the measured snapshot; its exact line/runtime effect is unmeasured. |
| S4 | Nice to have | Fixed in report: raw mode reads cell attributes through both Roo's decoder and unused Calamine `get_attrs!` results. No parser tuning was added. |

S1/S2 reproduction and verification output is in [validation.txt](validation.txt).
The fixes change CI/configuration and documentation only. Production parser,
counts, benchmark samples and all original test expectations remain intact.

## Round 2

Pending: recheck changed files and the round-1 ledger. The branch is not yet
presented as independently approved.
