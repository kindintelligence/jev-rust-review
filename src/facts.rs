//! Short documented facts added to a request's `state` when their gate
//! matches the code. TypeSafe's Models page recommends putting reference
//! material in `state` rather than relying on the model to recall it. Each
//! fact comes from the official documentation (see DESIGN.md) and must stay
//! literal. Like question gates, fact gates must not open on diff markers.

use regex::RegexSet;
use std::sync::LazyLock;

/// Most facts sent with one request.
const MAX_FACTS: usize = 5;

pub static FACTS: &[(&str, &str)] = &[
    (
        r"select!",
        "In `tokio::select!`, the branches that lose the race are dropped (cancelled). `read_exact`, `read_to_end`, `read_to_string`, `write_all`, `Mutex::lock`, `RwLock::read`, `RwLock::write`, `Semaphore::acquire` and `Notify::notified` are not cancellation safe: cancelling them can lose data or queue position. `recv` on channels, `accept`, `read`, `write` and `tokio::time::sleep` are cancellation safe.",
    ),
    (
        r"\.await",
        "A `std::sync::MutexGuard` or `RwLock` guard is not released by `.await`; it stays locked until it is dropped at the end of its scope or by `drop(guard)`.",
    ),
    (
        r"\bas\s+(u8|u16|u32|i8|i16|i32|usize|isize|u64|i64)\b",
        "An `as` cast between integer types silently truncates or wraps values that do not fit the target type; `TryFrom` reports the overflow instead.",
    ),
    (
        r"untagged",
        "With `#[serde(untagged)]`, serde tries the variants in declaration order and uses the first one that deserializes. Unknown fields are ignored by default and `Option` fields may be missing, so a later variant's data can match an earlier variant.",
    ),
    (
        r"get_unchecked|from_raw_parts|transmute|\*mut |\*const ",
        "`get_unchecked`, `slice::from_raw_parts` and raw pointer dereferences perform no bounds or validity checks; an out-of-range index or invalid pointer is undefined behaviour even if the result is never used.",
    ),
    (
        r"blocking_send|blocking_recv|blocking_lock",
        "Tokio's `blocking_send`, `blocking_recv` and `blocking_lock` panic when called from inside an async context.",
    ),
    (
        r"block_in_place",
        "`tokio::task::block_in_place` panics when called on a `current_thread` runtime.",
    ),
    (
        r"\.layer\(",
        "In Axum, with repeated `Router::layer` calls the layer added last runs first on the request; with `tower::ServiceBuilder`, layers run top to bottom.",
    ),
    // The facts below were added on 2026-09-21 from the documentation of
    // each item (DESIGN.md §1 lists the pages). Each states behaviour that
    // is written in prose and in no type.
    (
        r"select!|timeout\(",
        "`tokio::sync::mpsc::Sender::send` loses its message when cancelled: if a `select!` branch or a `timeout` drops the `send` future before it completes, the message is dropped and lost. This also holds when the branch awaits a function that awaits `send`. `Sender::reserve` followed by `Permit::send` does not lose the message.",
    ),
    (
        r"notify_waiters|\.notified\(\)",
        "`tokio::sync::Notify::notify_waiters` wakes only the `Notified` futures that already exist and stores no permit, unlike `notify_one`. A `Notified` future receives `notify_waiters` wakeups from the moment `notified()` creates it. Code that checks a condition and only then calls `notified()` misses a `notify_waiters` call made in between, so the `notified()` future must be created before the condition is checked.",
    ),
    (
        r"layer\(",
        "In Axum, `Router::layer` and `Router::route_layer` apply the middleware only to routes that were added before the call. A route added after the call does not get the middleware.",
    ),
    (
        r"BufWriter",
        "Dropping a `std::io::BufWriter` tries to flush its buffer but ignores any error, so a failed final write is lost silently; `flush` must be called before the drop to see the error. `tokio::io::BufWriter` does not flush on drop at all.",
    ),
    (
        r"size_hint",
        "`Iterator::size_hint` is not trusted: a safe iterator may report wrong bounds. `unsafe` code must not rely on it, for example to skip a bounds check or to size a buffer that it then writes without checking.",
    ),
    (
        r"spawn\(",
        "Dropping a `tokio::task::JoinHandle` detaches the task: it keeps running, and its panic or error is never observed. `JoinHandle::abort`, or dropping a `JoinSet`, stops it.",
    ),
    (
        r"use_signal|Signal<|\.read\(\)",
        "In Dioxus 0.7, holding a signal's `.read()` guard while writing the same signal panics with an already-borrowed error, and signal guards must not be held across `.await`.",
    ),
];

static GATES: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new(FACTS.iter().map(|(pattern, _)| *pattern))
        .expect("fact gates are compiled by the `gates_compile` test")
});

/// Facts whose gate matches `text`, in declaration order, at most five.
pub fn facts_for(text: &str) -> Vec<&'static str> {
    GATES
        .matches(text)
        .into_iter()
        .filter_map(|i| FACTS.get(i).map(|(_, fact)| *fact))
        .take(MAX_FACTS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_compile() {
        assert_eq!(GATES.len(), FACTS.len());
    }

    #[test]
    fn facts_gate_on_code_not_markers() {
        assert!(facts_for("+\n-\n>+\n -\n+    let x = y;\n").is_empty());
        let select = facts_for("+    tokio::select! { r = s.read_exact(&mut b) => {} }");
        assert!(select.iter().any(|f| f.contains("cancellation safe")));
        let has = |code: &str, needle: &str| facts_for(code).iter().any(|f| f.contains(needle));
        assert!(has("+    self.wake.notified().await;", "stores no permit"));
        assert!(has(
            "+        .route_layer(from_fn(require_admin))",
            "added before the call"
        ));
        assert!(has(
            "+    let mut out = BufWriter::new(file);",
            "ignores any error"
        ));
        assert!(has(
            "+    tokio::select! { () = forward(&tx, e) => {} }",
            "loses its message"
        ));
        assert!(
            facts_for("+    let n = x as u16;")
                .first()
                .is_some_and(|f| f.contains("truncates"))
        );
    }
}
