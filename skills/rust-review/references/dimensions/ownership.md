# Ownership and borrowing

Good APIs and code do not force needless allocation or copying. Their lifetimes reflect a sound design rather than a workaround.

## Look for

- **Needless ownership transfer.** The function only reads a parameter typed `String`, `Vec<T>` or `PathBuf`. That forces callers to allocate or give up their value. `&str`, `&[T]`, `&Path`, or `impl AsRef<Path>` would do.
- **Clones that are plausibly unnecessary and costly.** Examples:
  - cloning a large `Vec` or `String` inside a loop;
  - `.to_vec()` only to iterate;
  - `.clone()` to work around a borrow that a narrower scope would satisfy.
- **References held longer than needed**, which force later code into clones or `RefCell`.
- **Lifetime workarounds that hint at a design problem.** Examples:
  - `'static` bounds added to silence an error;
  - `Box::leak` in a non-startup path.

## Do not flag

- `Arc::clone`, `Rc::clone`, or `.clone()` on an `Arc`/`Rc`. These are reference-count bumps.
- Clones of small `Copy`-like values, short strings in cold paths, or data moved into a spawned task or thread. Moving across a task boundary requires ownership.
- A function that stores, moves, or mutates the owned argument. Owning is correct there.
- Clones in tests, examples, and one-shot setup code.

A `.clone()` is only interesting with a plausible argument against it. State explicitly whether it is unnecessary, expensive on a hot path, or hiding an ownership-design problem.

## Evidence that makes it a finding

**Both** of these must hold:

- The value is large, or the copy is repeated (in a loop or per request).
- A concrete alternative compiles. For example: "borrow `&self.items` instead; nothing after line 40 mutates it".
