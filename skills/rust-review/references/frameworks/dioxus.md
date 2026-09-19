# Dioxus profile (targets Dioxus 0.7.x; checked against the 0.7 docs, stable 0.7.10)

Load this only when the project facts list `dioxus`. Dioxus changed substantially between 0.5, 0.6 and 0.7. Check the version in `Cargo.toml`, and do not apply this guidance to older versions.

## Signals and reactive ownership

- `use_signal(|| v)` returns a `Signal<T>` (`Copy`), owned by the component and disposed on unmount.
  - Read with `.read()` or `sig()` (for `Clone` types), or `.cloned()` / `.with(..)`.
  - Write with `.write()`, `.set(v)` or `.with_mut(..)`.
  - `Signal::new` outside hooks can grow memory until the owner drops.
- **What subscribes.** Reading a signal during a component render subscribes that component. This applies to `.read()` and any method built on it. The same holds inside `use_memo`, `use_effect` and `use_resource` closures. `.peek()` reads **without** subscribing. A render that reads nothing does not re-run on change.
- **`ReadOnlySignal` is deprecated** in 0.7 in favour of `ReadSignal` (removal is planned for 0.8). Components can take `ReadSignal<T>` / `WriteSignal<T>`.
- **Borrow panics.** Holding a `.read()` guard while writing the same signal panics at runtime ("already borrowed"). Example: `for x in list.read().iter() { list.write().push(..) }`. Order operations so the guards do not overlap.
- **Never hold `.read()`/`.write()` guards across `.await`.** The docs point to Clippy's `await_holding_refcell_ref` lint.

## Hooks

- Hooks (`use_*`) must be called in the same order on every render. They must not appear:
  - in conditionals, loops, closures, event handlers, or other hooks' init closures;
  - after an early return.
- `use_hook(init)` runs `init` once and returns a clone of the stored value.

## Props and memoisation

- `#[component]` functions must be named in PascalCase (or contain an underscore), return `Element`, and take props that are `PartialEq + Clone`. `PartialEq` decides whether a child re-renders, so expensive-to-compare props belong behind a `ReadSignal`.
- Useful attributes: `#[props(default)]`, `#[props(into)]`, `#[props(!optional)]`. `Option<T>` props default to `None`.

## Derived state and effects

- Prefer `use_memo` for derived values. It notifies dependents only when the result changes (by `PartialEq`).
- `use_effect` runs after render and re-runs when signals it read change. **An effect that writes a signal it also reads loops forever.** Read that signal with `.peek()`. The docs prefer event handlers over effects for most state changes.
- Plain props or locals captured by these closures are **not tracked**. Wrap them with `use_reactive(..)`, or the closure keeps a stale value.

## Resources and tasks

- `use_resource` re-runs, cancelling the in-flight future, when a signal it reads changes. Its value is `None` while restarting, and its output is not memoised. Its futures must be cancel-safe.
- `use_future` spawns on first render and does not run on the server. `spawn(fut)` returns a `Task` that is cancelled on unmount. `spawn_forever` survives unmount; flag it where unmount cleanup was intended.
- Newer hooks: `use_action` (its `.call()` cancels the pending task) and `use_loader` (for `Result` futures; works with Suspense and ErrorBoundary).
- `use_server_future(..)` must be used with `?` to suspend. It tracks **only signals read in the closure, before the `async` block**. Reads inside the async block do not re-run it.
- Start all fetches before any conditional return, to avoid waterfalls.

## Server functions

- 0.7 adds `#[get("/api/x/{id}?q")]`, `#[post]`, `#[put]`, `#[delete]` and `#[patch]` with Axum-style paths. `#[server]` remains for anonymous endpoints.
- Server-only extractors go in the macro (`#[post("/login", auth: Session)]`).
- Functions must be `async` and return `Result<T, E>`. Payloads are JSON by default in 0.7.
- **Server functions are public HTTP endpoints.** Validate every argument and check authorisation inside them. Never rely on the UI calling them correctly.

## Rerenders

- Look for excess rerenders. A large signal read at the top of a parent re-renders every child that does not memoise.
- Stores (`#[derive(Store)]`, `use_store`) give per-field and per-entry reactivity for collections. They are an improvement over `Signal<HashMap<..>>` when many children watch one entry each.
