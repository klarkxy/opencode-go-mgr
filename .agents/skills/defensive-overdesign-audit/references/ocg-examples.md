# Historical OCG examples

Use only when concrete examples help calibrate a defense audit. These commits are historical evidence, not current runtime claims.

## Calibrate from the OCG cleanup batch

The repository batch `d35813cc^..bc1079ec` demonstrates the boundary:

- `d35813cc` removed repeated schema-number assertions, source-text policing,
  and an unused contract fixture while retaining migration and live catalog
  behavior.
- `862391b0` removed thousands of lines of `syn` and text-based architecture
  police plus their exclusive dev-dependencies.
- `dd186eb1` moved tests into sibling modules without deleting assertions; it
  was a readability change, not a defensive-overdesign cut.
- `baee80b8` extracted a shared fallback harness but retained error variants
  with distinct breaker, fallback, status, replay, and persistence outcomes.
- `ee546fcd` removed workflow-shape meta-tests and split slow release tooling
  from the fast web loop while preserving real release helpers and smoke.
- `ab702050` deleted documentation-shape and duplicate frontend assertions,
  while retaining WCAG, localization parity, redaction, and the only available
  UI regression net.
- `bc1079ec` removed drifting hand-maintained requirement maps and recorded the
  behavior-first testing convention.

The lesson is not "delete defensive tests." Delete self-maintained mirrors;
retain distinct observable consequences.

