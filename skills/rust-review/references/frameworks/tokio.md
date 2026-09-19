# Tokio profile (tokio 1.x, checked against 1.53 docs)

Load this only when the project facts list `tokio`.

## Runtime flavour

- `#[tokio::main]` defaults to the multi-threaded runtime. `flavor = "current_thread"` runs everything on one thread: any blocking call stalls every task.
- `tokio::task::block_in_place` **panics on a current_thread runtime**, and blocks other futures in the same task (`join!`/`select!` siblings).
- Creating a runtime or calling `Runtime::block_on` from inside async code panics ("Cannot start a runtime from within a runtime").

## `spawn` versus `spawn_blocking`

- Blocking or CPU-heavy work belongs in `spawn_blocking`. For long-running blocking loops, a dedicated `std::thread` is better: the blocking pool defaults to 512 threads, and extra work queues.
- `spawn_blocking` tasks **cannot be aborted** once started, and runtime shutdown waits for them.
- `blocking_send`, `blocking_recv` and `blocking_lock` **panic inside async context**.

## `JoinHandle` ownership

- Dropping a `JoinHandle` detaches the task; its panic and result are lost. Prefer `JoinSet` for groups, since dropping a `JoinSet` aborts all its tasks, or keep and await the handles.
- `JoinSet::join_next` is cancel-safe. `abort_all` still needs `join_next` to observe completion.

## `select!` cancellation safety

- Losing branches are dropped. **Not cancel-safe:**
  - `AsyncReadExt::read_exact`, `read_to_end`, `read_to_string`;
  - `AsyncWriteExt::write_all`;
  - `Mutex::lock`, `RwLock::read`/`write`, `Semaphore::acquire`, `Notify::notified`. These lose their queue position.
- **Cancel-safe:**
  - `mpsc`/`broadcast` `recv`, `watch::Receiver::changed`;
  - `TcpListener::accept`;
  - `AsyncReadExt::read` and `read_buf`;
  - `tokio::time::sleep`;
  - stream `next()`.
- In a loop, create the non-cancel-safe future once, `tokio::pin!` it, and poll it by `&mut`, or move the work into its own task.
- `biased;` polls branches in order. Without an `else` branch, `select!` panics if all branches are disabled.

## Sync versus async mutex

- The Tokio docs: it is "ok and often preferred" to use `std::sync::Mutex` in async code, as long as the guard is never held across `.await`. `tokio::sync::Mutex` is for locks that must be held across `.await`, such as around an I/O resource. It is slower and FIFO-fair.
- Flag an async mutex only when it is never held across an await and sits on a hot path. Flag a std mutex whenever its guard is live at an await.

## Channels and backpressure

- `mpsc::channel(n)` is bounded and gives backpressure. `unbounded_channel` can grow without limit when producers outpace consumers.
- Bridging sync code to async: use an unbounded Tokio channel. Bridging async to sync: use a std or crossbeam channel.

## Graceful shutdown

The pattern is: `tokio::signal::ctrl_c()` in a `select!` → cancel a `tokio_util::sync::CancellationToken` → tasks observe `token.cancelled()` (cancel-safe) → `TaskTracker::close()` followed by `.wait().await`. Look for long-running loops with no way to observe shutdown.
