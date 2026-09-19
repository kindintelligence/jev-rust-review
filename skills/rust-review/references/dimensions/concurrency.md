# Send/Sync and concurrency

The compiler rules out data races. It does not rule out race conditions, lock misuse, or weak atomics.

## Look for

- **Check-then-act.** A value checked under one lock acquisition and acted on under another. For example, `contains_key` then `insert` with two `lock()` calls. The same applies to filesystem races (`exists()` then `create`) and to atomics (`load` then `store`).
- **Over-broad critical sections.** A lock held across I/O, logging, callbacks, or another lock acquisition. That causes contention, and deadlock if lock order varies.
- **Lock ordering.** Two locks acquired in different orders on different paths.
- **Atomics:**
  - `Ordering::Relaxed` on a flag that publishes other data (needs `Release`/`Acquire`);
  - `load` followed by `store` where `fetch_add`, `compare_exchange` or `fetch_update` is required.
- **`unsafe impl Send` / `unsafe impl Sync`** on types containing `Rc`, `Cell`, `RefCell`, raw pointers, or thread-affine handles. The justification must explain why cross-thread access is sound.
- **Reflexive `Arc<Mutex<T>>`.** Comment on it only when a clearly simpler option exists. The options are ownership by a single task (message passing) or a plain `&`.

## Do not flag

- Relaxed counters used only for statistics.
- A single lock acquisition that covers both the check and the act.
- `Arc<Mutex<T>>` where shared mutable state is genuinely needed.

## Evidence that makes it a finding

An interleaving, written as steps. For example: "T1 checks, and the name is free; T2 checks, and the name is free; T1 inserts; T2 inserts, and T1's registration is overwritten."
