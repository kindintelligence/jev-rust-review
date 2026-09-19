# Performance

Report only what plausibly matters: hot paths, per-request work, or cost that grows with input size. Do not report micro-optimisations for their own sake.

## Look for

- **Allocation in hot loops.** A per-iteration `format!`, `to_string`, `clone`, `collect` or `Vec::new` whose value could be hoisted or reused.
- **Accidental quadratic behaviour.**
  - `Vec::contains` or `iter().find` inside a loop over the same size of data;
  - `remove(0)` in a loop;
  - string building with `+` in a loop.
- **Repeated parsing or conversion.** `Regex::new` in a function called per item; re-parsing the same config or JSON.
- **Needless `collect`** into a `Vec` just to iterate again, or to call `.len()`.
- **Lock contention.** A global mutex on a per-request path held across work.

## Do not flag

- Code that runs once at startup, in tests, or on bounded small inputs.
- Iterator-versus-loop style. That is an idiom question, not performance.
- Anything where the "fix" makes the code harder to read for no measurable gain.

## Evidence that makes it a finding

The growth. For example: "for n requests each scanning `sessions` linearly, this is O(n²); with 10k sessions that is 10⁸ comparisons per batch."

A benchmark and profile you ran also counts.
