---
name: defensive-overdesign-audit
description: Classify one suspected defensive mechanism as necessary protection, removable overdesign, or replacement-first coverage. Use for focused questions about defensive tests, validators, compatibility layers, retries, fallbacks, guards, requirement maps, CI gates, 过度防御设计, 防御性编程清理, test-suite entropy, or architecture-police tests. Do not use for broad codebase simplification or implementation of accepted cuts; use simplify-codebase for those. Do not use as authorization to weaken authentication, validation, lifecycle, compatibility, accessibility, data-loss, or release boundaries.
---

# Defensive Overdesign Audit

Treat every defense as a claim about a concrete failure. Keep defenses that
uniquely expose or contain a current failure; remove or merge defenses that
only mirror implementation shape, repeat another owner, or maintain a second
truth that changes in lockstep with the code.

This skill is a narrow decision gate, not a second general simplification
framework. It classifies a suspected defense. If the user explicitly
authorizes implementation, pass accepted cuts to `simplify-codebase` in Change
mode for editing, recovery, and validation.

## Set the boundary

Default to a read-only audit. Inspect only the named mechanism and the paths
needed to establish its callers, failure window, and stronger existing owner.
Do not build a full repository dependency graph unless dynamic loading,
external consumers, or persisted formats make the focused result unknowable.

Read repository instructions and version-control state first. Treat code,
executed workflows, public artifacts, and current acceptance requirements as
stronger evidence than stale prose or commit-message claims.

For each candidate, answer:

1. What concrete invalid state, user-visible failure, security event, data
   loss, incompatible artifact, or stuck lifecycle does it claim to prevent?
2. Which current caller, input, process, stored value, external consumer, or
   acceptance requirement can reach that failure?
3. What observable signal distinguishes the defense succeeding from the code
   merely retaining a particular shape?
4. Does another owner already cover the same input, transition, output,
   side effect, and failure window?
5. What behavior-level proof remains after the proposed cut?
6. Which exclusive fixtures, helpers, dependencies, documentation, or CI
   wiring become dead with it?

Lack of a repository-local caller is not enough when a surface may be public,
dynamic, generated, persisted, or consumed by another process.

## Classify without scores

Use one of three decisions. Do not invent risk weights, waiver databases, or
rule registries.

### GREEN CUT

Remove or merge when evidence shows no unique observable protection. Common
forms are:

- a test that copies a current constant, enum, table, token list, or version
  while migration, schema, or public behavior is tested elsewhere;
- `include_str!`, `read_to_string`, regex, AST, or YAML tests that police
  imports, re-exports, module membership, debug seams, workflow spelling, or
  documentation phrases, but cannot observe a runtime or artifact failure;
- the same invariant asserted by several suites, where one test closest to the
  public or domain owner covers the complete behavior;
- a hand-maintained requirement-to-test matrix that duplicates names and has
  no current external compliance consumer;
- a meta-test that parses or replays fragments of a script which the real CI
  path already executes;
- a fixture, helper, or test-only dependency with no remaining consumer;
- a test case that is a strict subset of another case: identical input,
  transition, output, side effects, and failure window.

After selecting a cut, identify the surviving behavioral owner. A deletion
without a surviving proof is not green.

### YELLOW HOLD

Do not remove yet when the defense looks brittle or duplicated but one fact is
missing. Typical cases are:

- a source-grep UI test is the only coverage of keyboard, ARIA, redaction, or
  another user-visible behavior; replace it with mount, browser, or
  accessibility coverage first;
- a compatibility surface has no in-repository caller, but external, dynamic,
  generated, or stored consumers have not been ruled out;
- a traceability map may be required by a regulator or external auditor;
- a snapshot or golden file might represent a published wire or artifact
  contract rather than internal formatting;
- two guards appear to overlap, but they may cover different owners,
  transitions, or failure windows.

Name the exact fact or replacement test needed to decide the candidate.

### RED KEEP

Keep a defense when it protects a reachable boundary or produces a distinct
post-condition. Ordinary simplification must not weaken:

- authentication, authorization, secret handling, redaction, URL or outbound
  trust validation, sandboxing, and privilege isolation;
- CAS, atomic writes, migrations, rollback, crash recovery, data-loss
  prevention, or persisted-format compatibility;
- cancellation, shutdown, cleanup, quiescence, process containment, partial
  output, retry safety, replay prevention, or unknown-outcome accounting;
- public protocol, wire schema, generated artifact, signing, packaging, or
  release smoke behavior;
- accessibility requirements;
- error cases that share a status or setup but differ in breaker state,
  fallback choice, replay action, external calls, persistence, accounting, or
  the status returned to the caller.

A compiler, linter, schema generator, dependency rule, or security scanner is
not automatically overdesign merely because it reads source. Ask whether it
derives independent semantics and catches a real prohibited outcome, rather
than matching the repository's current spelling.

## Choose the smallest action

Classification and action are separate:

- Repeated setup usually calls for a shared harness, not fewer behavior cases.
- A slow specialized suite usually belongs in a change-gated or release lane,
  not in the default fast loop and not in the trash.
- A large inline test module may move to a sibling file for readability; that
  is organization, not deletion of a defense.
- Duplicate tables should become one canonical owner plus property-level
  checks, not another generated mirror unless the generated artifact is itself
  public.
- Removing a candidate includes its exclusive fixtures, helpers, dependencies,
  copied maps, and obsolete documentation. Do not leave a dead control surface.

Do not create an AST or keyword scanner to find these patterns, do not make the
skill a CI blocker, and do not auto-delete. Search results are leads; the
failure and consumer evidence decides.

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

## Report the decision

Lead with the result, then use one record per candidate:

```text
Mechanism:
Claimed failure:
Boundary and consumers:
Decision: GREEN CUT | YELLOW HOLD | RED KEEP
Action:
Surviving acceptance behavior:
Minimum verification:
Evidence and confidence:
```

Finish with:

- the total candidates in each class;
- transitive files or dependencies affected by accepted cuts;
- important rejected deletions and why they remain defenses;
- unresolved facts and the next evidence needed.

For an authorized change, hand only `GREEN CUT` records to
`simplify-codebase`. Preserve the `YELLOW HOLD` and `RED KEEP` boundaries as
explicit constraints in that implementation brief.
