# ENGINEERING_TASTE.md

Purpose:

Approximate the owner's engineering taste.
When uncertain, optimize for predictability and minimal disruption.

## Core Philosophy

Prefer:

- Small incremental changes
- Boring, understandable solutions
- Shipping over theoretical elegance
- Explicit state over hidden magic
- Consistency over novelty

Avoid:

- Large rewrites
- Speculative abstractions
- Unrelated refactors
- New infrastructure without a proven pain point
- Silent behavior changes

---

## Decision Priority

When tradeoffs exist, prioritize in this order:

1. Preserve existing behavior
2. Keep scope small
3. Keep implementation simple
4. Reduce maintenance burden
5. Improve architecture
6. Improve performance

Do not sacrifice higher priorities for lower ones.

---

## Change Style

Prefer:

- Minimal patches
- Localized changes
- Existing patterns in the codebase
- Incremental migration over replacement

Avoid:

- Rewriting working systems during unrelated tasks
- Framework churn
- Premature generalization

Rule of thumb:

"If it already works, improve carefully."

---

## Scope Control

Only solve the requested problem.

Do not:

- Expand requirements
- Add future-proofing unless explicitly requested
- Introduce unrelated cleanup
- Refactor adjacent systems "while here"

Good:

- Small fix for the requested task

Bad:

- Turning a small task into architecture work

---

## Communication Style

Work in short loops.

Prefer:

- Small deliverables
- Frequent checkpoints
- Showing diffs before large changes
- Explaining tradeoffs briefly

When uncertain:

Ask or stop and report.

Never make large architectural decisions silently.

---

## Decision Heuristics

When uncertain:

Prefer minimal change.

When two options are similar:

Choose the more boring solution.

Before adding dependency/infrastructure:

Show the concrete pain point first.

Before refactoring:

Explain why existing structure is insufficient.

When tempted to improve unrelated code:

Do not.

---

## Red Flags

Stop and ask before:

- Replacing working systems
- Changing build/release pipelines
- Introducing databases/services/frameworks
- Touching many unrelated files
- Changing architecture
- Increasing operational complexity

---

## Success Criteria

Good work feels:

- Predictable
- Small in scope
- Easy to review
- Easy to revert
- Easy to maintain
