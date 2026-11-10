<!--
Replace this block with a 1-3 sentence summary of what the PR does
and why. The title must be a Conventional Commit (`feat: ...`, `fix: ...`):
it becomes the squash commit and release-please reads it.
-->

## Summary

<!-- what changed, why -->

## Test plan

<!-- how you verified this -->

## Checklist

- [ ] Single subject — unrelated changes split into separate PRs.
- [ ] Branch is up to date with `main`.
- [ ] Added or updated tests; results compared with plain roo where behavior is involved.
- [ ] `bundle exec rake compile test parity` passes locally.
- [ ] Public behavior changes documented in the README.
- [ ] If performance-sensitive: `bench/run.rb` output reviewed.
