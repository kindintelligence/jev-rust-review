# Testing

Jev does not judge test adequacy. It cannot trace coverage across files. The evaluation tool reports facts instead: `project.tests.diff_touches_tests`, and which changed units are test code. Jev asks one local question, `testing.weak_assertion`: whether a changed test asserts on what it exercises. The adequacy judgment is yours.

## Look for

- **Changed behaviour without a changed or added test.** Start with bug fixes. A fix without a regression test tends to regress.
- **Tests that do not assert the behaviour.**
  - A test that calls the function and asserts nothing.
  - A test that asserts only `is_ok()` on a result whose value matters.
  - A test that would pass with the change reverted.
- **Test types that fit the change.** Recognise each kind:
  - unit tests (`#[cfg(test)]`);
  - integration tests (`tests/`);
  - doc tests;
  - property tests (`proptest`, `quickcheck`);
  - compile-fail and UI tests (`trybuild`);
  - snapshot tests (`insta`).
- **Async tests.** A test on the wrong runtime flavour. A test that relies on timing (`sleep`) where `tokio::time::pause` would make it deterministic.

## Do not flag

- Refactors that keep behaviour and are covered by existing tests.
- Missing tests for trivial glue.
- A missing property test when there is no clear invariant. Suggest property tests only where a real invariant exists (round trips, ordering, idempotence).

## Evidence that makes it a finding

The specific behaviour that changed, and the absence of any test exercising it. Use Grep for the function name under `tests/` and `#[cfg(test)]`, and state what you searched.
