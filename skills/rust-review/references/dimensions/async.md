# Async

Async bugs are rarely visible in a diff and often show up only under load. **Detect the runtime from the project facts; never assume Tokio.** If `async_runtimes` is empty, the crate may be runtime-agnostic, and runtime-specific advice does not apply.

## Look for

- **Blocking in async context.** Examples:
  - `std::thread::sleep`;
  - `std::fs` or `std::net` I/O;
  - blocking HTTP clients;
  - `Mutex::lock` on a contended std mutex;
  - long CPU loops;
  - synchronous database drivers.

  Each one stalls a worker thread and every task scheduled on it.
- **Sync guards across `.await`.** A `std::sync::MutexGuard`, `RwLock` guard, `RefCell` borrow, or `parking_lot` guard stays held while the future is suspended. Other tasks block, and re-entrancy deadlocks. The fix is usually to scope the guard in a block that ends before the await. An async mutex is the right fix only when the lock genuinely must span the await.
- **Cancellation safety.** In `select!`, losing branches are dropped. A future that has consumed part of a stream (`read_exact`, `read_to_end`, `write_all`) or queued for a lock loses that progress. Accept loops in `select!` need cancel-safe futures, or futures pinned outside the loop and polled by `&mut`.
- **Detached tasks.** A `spawn` whose handle is dropped has no owner. Its panics and errors vanish, and shutdown cannot stop it.
- **Unbounded concurrency or buffering.** Unbounded channels fed faster than they drain, and one task per input item with no semaphore or `buffer_unordered(n)`.
- **Accidental sequentialisation.** Independent awaits in a loop that could run with `join_all` or `JoinSet`. Report this only when latency plausibly matters.
- **Channel deadlocks and leaked tasks.** A sender kept alive so `recv()` never returns `None`, and bounded channels in both directions between two tasks.
- **`Pin` misuse.** Moving a value after pinning, or a manual `Future` implementation that breaks pinning. Pair with the unsafe reference if `unsafe` is involved.
- **Async where sync would be simpler.** An `async fn` with no await points, adding runtime coupling for nothing.

## Do not flag

- A std `Mutex` held briefly with no await inside the critical section. This is the recommended pattern: the Tokio docs say std mutexes are often preferred.
- Sequential awaits where order matters or the count is tiny.
- Detached tasks that handle their own errors and are meant to run for the process lifetime. Only a missing shutdown path is worth mentioning.
- Blocking calls in `main` before the runtime starts, or inside `spawn_blocking`.

## Evidence that makes it a finding

- For a guard: the binding line, the await line, and why the guard is still live.
- For cancellation: the branch that can lose, and what state is lost.
- For blocking: the call, and the async function it runs in.
