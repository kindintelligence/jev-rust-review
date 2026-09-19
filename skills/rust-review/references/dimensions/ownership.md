# Ownership and borrowing

The goal is APIs and code that do not force needless allocation or copying, and lifetimes that reflect a sound design rather than a workaround.

## Look for

- **Needless ownership transfer.** A parameter typed `String`, `Vec<T>` or `PathBuf` that the function only reads forces callers to allocate or give up their value. `&str`, `&[T]`, `&Path`, or `impl AsRef<Path>` would do.
- **Clones that are plausibly unnecessary and costly.** Examples: cloning a large `Vec` or `String` inside a loop, `.to_vec()` just to iterate, `.clone()` to work around a borrow that a narrower scope would satisfy.
- **References held longer than needed**, which force later code into clones or `RefCell`.
- **Lifetime workarounds that hint at a design problem.** For example, `'static` bounds added to silence an error, or `Box::leak` in a non-startup path.

## Do not flag

- `Arc::clone`, `Rc::clone`, or `.clone()` on an `Arc`/`Rc`. These are reference-count bumps.
- Clones of small `Copy`-like values, short strings in cold paths, or data moved into a spawned task or thread. Moving across a task boundary requires ownership.
- A function that stores, moves, or mutates the owned argument. Owning is correct there.
- Clones in tests, examples, and one-shot setup code.

A `.clone()` is only interesting when there is a plausible argument that it is unnecessary, expensive on a hot path, or hiding an ownership-design problem. Make that argument explicitly.

## Evidence that makes it a finding

The value is large or the copy is repeated (in a loop or per request), **and** there is a concrete alternative that compiles: "borrow `&self.items` instead; nothing after line 40 mutates it".
