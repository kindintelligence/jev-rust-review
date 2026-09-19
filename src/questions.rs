//! Every Jev question and every default threshold lives in this file, so the
//! part of the system that most needs human review can be read in one place.
//!
//! Design rules, from the Jev 1.13 jaggedness page:
//! - literal, single-hop instructions that name the state field (`code`);
//! - Noul criteria mirror the instruction (`true` = the defect is present);
//! - no counting, arithmetic, line numbers, or "does it compile";
//! - lexical gates in code decide which questions apply to a unit;
//! - every flag rule reads exactly one answer (no consistency assumptions).

use crate::rust_project::Role;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Correctness,
    Ownership,
    TypeDesign,
    ErrorHandling,
    Async,
    Concurrency,
    Unsafe,
    Ffi,
    Performance,
    Idiom,
    Api,
    Macros,
    Serde,
    Security,
    Testing,
    Cargo,
}

impl Dimension {
    pub const ALL: [Dimension; 16] = [
        Dimension::Correctness,
        Dimension::Ownership,
        Dimension::TypeDesign,
        Dimension::ErrorHandling,
        Dimension::Async,
        Dimension::Concurrency,
        Dimension::Unsafe,
        Dimension::Ffi,
        Dimension::Performance,
        Dimension::Idiom,
        Dimension::Api,
        Dimension::Macros,
        Dimension::Serde,
        Dimension::Security,
        Dimension::Testing,
        Dimension::Cargo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Dimension::Correctness => "correctness",
            Dimension::Ownership => "ownership",
            Dimension::TypeDesign => "type_design",
            Dimension::ErrorHandling => "error_handling",
            Dimension::Async => "async",
            Dimension::Concurrency => "concurrency",
            Dimension::Unsafe => "unsafe",
            Dimension::Ffi => "ffi",
            Dimension::Performance => "performance",
            Dimension::Idiom => "idiom",
            Dimension::Api => "api",
            Dimension::Macros => "macros",
            Dimension::Serde => "serde",
            Dimension::Security => "security",
            Dimension::Testing => "testing",
            Dimension::Cargo => "cargo",
        }
    }

    pub fn parse(s: &str) -> Option<Dimension> {
        let s = s.trim().to_ascii_lowercase().replace('-', "_");
        let s = match s.as_str() {
            "errors" | "error" | "panic" => "error_handling",
            "types" | "type" => "type_design",
            "borrowing" => "ownership",
            "perf" => "performance",
            "maintainability" | "style" => "idiom",
            "semver" | "api_compat" => "api",
            "tests" | "test" => "testing",
            "dependencies" | "deps" => "cargo",
            "soundness" => "unsafe",
            "send_sync" => "concurrency",
            other => other,
        }
        .to_string();
        Dimension::ALL.into_iter().find(|d| d.name() == s)
    }

    /// Triage bar: low where a miss is expensive (recall-oriented).
    pub fn default_triage_threshold(self) -> f64 {
        match self {
            Dimension::Unsafe | Dimension::Async | Dimension::Security | Dimension::Concurrency => {
                0.25
            }
            Dimension::Correctness | Dimension::ErrorHandling | Dimension::Ffi => 0.30,
            Dimension::Serde | Dimension::Api | Dimension::Macros | Dimension::Cargo => 0.35,
            Dimension::Ownership
            | Dimension::TypeDesign
            | Dimension::Performance
            | Dimension::Testing => 0.45,
            Dimension::Idiom => 0.60,
        }
    }

    /// Report bar after verification: high, because a false positive
    /// reaching the user is the expensive error (precision-oriented).
    pub fn default_report_threshold(self) -> f64 {
        match self {
            Dimension::Unsafe => 0.80,
            Dimension::Idiom | Dimension::TypeDesign => 0.80,
            _ => 0.70,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    Noul {
        yes: &'static str,
        no: &'static str,
    },
    /// Flags when the probability mass on levels `bad_from..` reaches the
    /// threshold.
    Score {
        levels: &'static [&'static str],
        bad_from: usize,
    },
    /// Flags when the probability mass on `flag` options reaches the
    /// threshold.
    Choice {
        options: &'static [(&'static str, &'static str)],
        flag: &'static [&'static str],
    },
}

impl Primitive {
    pub fn name(&self) -> &'static str {
        match self {
            Primitive::Noul { .. } => "noul",
            Primitive::Score { .. } => "score",
            Primitive::Choice { .. } => "choice",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    RustCode,
    Manifest,
}

#[derive(Clone, Copy, Debug)]
pub struct QuestionSpec {
    pub id: &'static str,
    pub dimension: Dimension,
    pub instructions: &'static str,
    pub primitive: Primitive,
    /// Groups of regexes. Every group must match somewhere in the unit's
    /// code for the question to be asked. Empty = always asked.
    pub gate: &'static [&'static [&'static str]],
    pub skip_roles: &'static [Role],
    pub unit: UnitKind,
    /// Overrides the dimension's triage threshold for this question.
    pub threshold: Option<f64>,
}

impl QuestionSpec {
    const fn noul(
        id: &'static str,
        dimension: Dimension,
        instructions: &'static str,
        yes: &'static str,
        no: &'static str,
    ) -> QuestionSpec {
        QuestionSpec {
            id,
            dimension,
            instructions,
            primitive: Primitive::Noul { yes, no },
            gate: &[],
            skip_roles: &[],
            unit: UnitKind::RustCode,
            threshold: None,
        }
    }
    const fn gate(mut self, gate: &'static [&'static [&'static str]]) -> Self {
        self.gate = gate;
        self
    }
    const fn skip(mut self, roles: &'static [Role]) -> Self {
        self.skip_roles = roles;
        self
    }
    const fn threshold(mut self, t: f64) -> Self {
        self.threshold = Some(t);
        self
    }
    const fn manifest(mut self) -> Self {
        self.unit = UnitKind::Manifest;
        self
    }
}

/// A framework profile is data: detection crates, extra questions, and the
/// reference file the reviewer loads when the profile is active.
#[derive(Debug)]
pub struct Profile {
    pub name: &'static str,
    pub detect_crates: &'static [&'static str],
    pub questions: &'static [QuestionSpec],
    pub reference: &'static str,
}

const NON_PROD: &[Role] = &[Role::Test, Role::Example, Role::Bench];
const NON_LIB: &[Role] = &[
    Role::Test,
    Role::Example,
    Role::Bench,
    Role::BuildScript,
    Role::Binary,
];
const TESTS: &[Role] = &[Role::Test];

use Dimension as D;
use QuestionSpec as Q;

/// Core questions, asked for every project (subject to gates and roles).
pub static CORE: &[QuestionSpec] = &[
    // ---- correctness -------------------------------------------------
    Q::noul(
        "correctness.logic",
        D::Correctness,
        "Do the changed lines in `code` (lines starting with `+`) compute a wrong result for some input, for example through an inverted condition, a wrong comparison operator, the wrong variable, or a missing case?",
        "A specific input or state makes the changed code produce a wrong result.",
        "The changed code produces the intended result for every input it can receive.",
    )
    .skip(NON_PROD),
    Q::noul(
        "correctness.bounds",
        D::Correctness,
        "Can an index, a slice range, or a length calculation in the changed lines of `code` go out of bounds or be off by one?",
        "Some reachable input makes an index or range exceed the valid bounds, or miss the first or last element.",
        "Every index and range stays within bounds and covers exactly the intended elements.",
    )
    .gate(&[&[r"\[", r"\.\.", r"len\(\)", r"split_at", r"get_unchecked"]])
    .skip(NON_PROD),
    Q::noul(
        "correctness.cast",
        D::Correctness,
        "Does a changed line in `code` use an `as` cast that can silently truncate, wrap, or change the sign of a value that can be out of range for the target type?",
        "A value that can realistically be out of range is converted with `as`.",
        "Every `as` cast converts values that always fit, or the truncation is clearly intended.",
    )
    .gate(&[&[r"\bas\s+(u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64|char)\b"]])
    .skip(NON_PROD),
    Q::noul(
        "correctness.overflow",
        D::Correctness,
        "Can integer arithmetic in the changed lines of `code` overflow or underflow for inputs the code can realistically receive, for example subtracting a larger unsigned value from a smaller one?",
        "Some realistic input makes an addition, subtraction, multiplication, or shift overflow or underflow.",
        "The arithmetic cannot overflow for realistic inputs, or it uses checked, saturating, or wrapping operations.",
    )
    .gate(&[
        &[r"\b(u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize)\b", r"len\(\)"],
        &[r"[^-]-\s*[\w(]", r"\+", r"\*\s*\w", r"<<"],
    ])
    .skip(NON_PROD),
    Q::noul(
        "correctness.wildcard",
        D::Correctness,
        "Does a changed `match` in `code` use a wildcard `_` arm on an enum where a variant added later would be handled wrongly by that arm without any compiler error?",
        "The wildcard arm would silently apply behaviour that is wrong for new variants.",
        "The wildcard arm is correct for any future variant, or the match is not over an enum.",
    )
    .gate(&[&[r"_\s*=>"]])
    .skip(NON_PROD)
    .threshold(0.45),
    Q::noul(
        "correctness.drop_order",
        D::Correctness,
        "Does the changed code in `code` drop a value earlier or later than the code relies on, for example a guard bound to `let _ =` that is dropped immediately, or a temporary that lives to the end of a statement?",
        "A value's drop point differs from what the surrounding code relies on.",
        "Every value lives exactly as long as the code relies on.",
    )
    .gate(&[&[r"let\s+_\s*=", r"\bdrop\(", r"impl\s+Drop", r"_guard", r"mem::forget", r"lock\(\)"]])
    .skip(NON_PROD),
    // ---- ownership ---------------------------------------------------
    Q::noul(
        "ownership.clone",
        D::Ownership,
        "Do the changed lines in `code` copy data that could have been borrowed instead, where the copy is large or runs inside a loop?",
        "An avoidable copy of a large value, or a copy repeated inside a loop.",
        "No avoidable copy, or the copy is cheap: an `Arc` or `Rc` clone, a small value, or data moved into a spawned task or thread.",
    )
    .gate(&[&[r"\.clone\(\)", r"\.to_owned\(\)", r"\.to_vec\(\)", r"\.to_string\(\)", r"String::from", r"\.collect"]])
    .skip(NON_PROD),
    Q::noul(
        "ownership.signature",
        D::Ownership,
        "Does a changed function signature in `code` take ownership of a `String`, `Vec`, `PathBuf`, or other owned value that the function only reads?",
        "The function only reads the owned argument, so callers must allocate or give up their value for no reason.",
        "The function stores, moves, or mutates the owned argument, or it takes a borrowed type already.",
    )
    .gate(&[&[r"fn\s+\w+[^{;]*:\s*(String|Vec<|PathBuf|Box<)"]])
    .skip(NON_PROD)
    // Tuned 2026-09-19 against the eval corpus: consumed Vec args scored 0.60.
    .threshold(0.65),
    // ---- type design -------------------------------------------------
    Q::noul(
        "type_design.loose_types",
        D::TypeDesign,
        "Does the changed code in `code` represent a fixed set of states or modes with booleans, strings, integers, or sentinel values where an enum or a dedicated type would prevent invalid values?",
        "Invalid combinations or values are representable and a simple enum or newtype would rule them out.",
        "The representation is appropriate, or a dedicated type would add more complexity than it removes.",
    )
    .gate(&[&[r"\bbool\b", r"String", r"&str", r"-1\b", r"Option<", r"\bu8\b|\bi32\b|\bu32\b"]])
    .skip(NON_LIB),
    // ---- error handling ------------------------------------------------
    Q::noul(
        "error_handling.swallowed",
        D::ErrorHandling,
        "Do the changed lines in `code` discard an error so that a failure goes unnoticed, for example `let _ =` on a `Result`, `.ok()`, `unwrap_or_default()`, or an `Err(_)` branch that does nothing?",
        "A failure is silently ignored and the program continues as if the operation succeeded.",
        "Every error is handled, returned, logged, or deliberately ignored where ignoring it is correct.",
    )
    .gate(&[&[r"let\s+_\s*=", r"\.ok\(\)", r"unwrap_or_default", r"unwrap_or\(", r"Err\(_\)", r"if\s+let\s+Ok", r"\.is_ok\(\)", r"_\s*=>"]])
    .skip(TESTS),
    Q::noul(
        "error_handling.panic",
        D::ErrorHandling,
        "Can the changed lines in `code` panic on input or state that the program does not control, for example through `unwrap`, `expect`, indexing, `panic!`, or `unreachable!`?",
        "A panic is reachable from external input, I/O results, or other state the program does not control.",
        "No reachable panic, or the panic enforces an invariant that the surrounding code has already checked or documented.",
    )
    .gate(&[&[r"unwrap\(", r"expect\(", r"panic!", r"unreachable!", r"todo!", r"unimplemented!", r"\[[^\]]+\]"]])
    .skip(NON_PROD),
    Q::noul(
        "error_handling.lossy",
        D::ErrorHandling,
        "Does the changed code in `code` convert or replace an error in a way that drops the original error's message, source, or context?",
        "The original error's information is discarded, so the caller cannot tell what actually failed.",
        "The original error is kept, wrapped, or chained, or it carries no useful information.",
    )
    .gate(&[&[r"map_err", r"\?", r"Box<dyn", r"anyhow!", r"Error", r"ok_or"]])
    .skip(TESTS),
    Q::noul(
        "error_handling.drop_panic",
        D::ErrorHandling,
        "Can a changed `Drop` implementation in `code` panic?",
        "The `drop` body contains an operation that can panic.",
        "The `drop` body cannot panic.",
    )
    .gate(&[&[r"impl[^{]*\bDrop\s+for"]]),
    // ---- async ---------------------------------------------------------
    Q::noul(
        "async.guard_across_await",
        D::Async,
        "In `code`, is a guard from `std::sync::Mutex`, `std::sync::RwLock`, `parking_lot`, or `RefCell` still alive at an `.await` point?",
        "A synchronous lock or borrow guard is held while the function awaits.",
        "Every such guard is dropped before any `.await`, or the lock is an async lock such as `tokio::sync::Mutex`.",
    )
    .gate(&[&[r"\.await"], &[r"lock\(\)", r"\.read\(\)", r"\.write\(\)", r"borrow", r"Mutex", r"RwLock"]]),
    Q::noul(
        "async.blocking_call",
        D::Async,
        "Does an `async` function or block in `code` call an operation that blocks the thread, such as `std::thread::sleep`, `std::fs` or `std::net` I/O, a blocking HTTP client, or a long CPU-bound loop, directly instead of through `spawn_blocking`?",
        "Blocking work runs directly on the async executor thread.",
        "All blocking work is offloaded, or the operation is non-blocking or trivially short.",
    )
    .gate(&[&[r"\basync\b"], &[r"thread::sleep", r"std::fs", r"fs::", r"File::", r"std::net", r"blocking", r"read_to_string", r"\.join\(\)", r"Command::new", r"stdin", r"\bfor\b", r"\bloop\b", r"\bwhile\b", r"sync_channel", r"\.recv\(\)"]]),
    Q::noul(
        "async.select_cancellation",
        D::Async,
        "In a `select!` in `code`, can a branch that loses the race be cancelled after it has consumed data or partly updated state, so that data is lost or state is left inconsistent?",
        "Cancelling a losing branch loses data or leaves state half-updated.",
        "Every branch is cancellation safe, or losing progress in it is harmless.",
    )
    .gate(&[&[r"select!"]]),
    Q::noul(
        "async.detached_task",
        D::Async,
        "Does the changed code in `code` spawn a task and drop its `JoinHandle`, so that nothing awaits the task, observes its errors or panics, or stops it on shutdown?",
        "A spawned task has no owner that awaits it, checks its result, or can stop it.",
        "Every spawned task is awaited, tracked in a set, or deliberately detached with its errors handled inside the task.",
    )
    .gate(&[&[r"spawn\("]])
    .skip(TESTS),
    Q::noul(
        "async.unbounded",
        D::Async,
        "Does the changed code in `code` allow work or buffering to grow without a limit, such as an unbounded channel that can be fed faster than it is drained, or one spawned task per input item with no cap?",
        "Memory or task count can grow without bound under load.",
        "Work and buffering are bounded, or the input size is small and fixed.",
    )
    .gate(&[&[r"unbounded", r"spawn\(", r"join_all", r"FuturesUnordered", r"buffer_unordered", r"for_each_concurrent"]])
    .skip(TESTS),
    Q::noul(
        "async.sequential_awaits",
        D::Async,
        "Does the changed code in `code` await independent operations one after another inside a loop where they could safely run concurrently?",
        "Independent awaits run strictly in sequence and could run concurrently.",
        "The awaits depend on each other, must be ordered, or are few enough that order does not matter.",
    )
    .gate(&[&[r"\.await"], &[r"\bfor\b", r"\bwhile\b", r"\bloop\b"]])
    .skip(NON_PROD)
    .threshold(0.65),
    // ---- concurrency -----------------------------------------------------
    Q::noul(
        "concurrency.check_then_act",
        D::Concurrency,
        "Does the changed code in `code` check shared state and then act on it in a separate step, so that another thread or task can change the state in between?",
        "The check and the action are not atomic with respect to other threads or tasks.",
        "The check and the action happen under one lock or one atomic operation, or the state is not shared.",
    )
    .gate(&[
        &[r"lock\(\)", r"\.read\(\)", r"\.write\(\)", r"\.load\(", r"contains", r"\.get\(", r"exists", r"is_some", r"is_none"],
        &[r"Mutex", r"RwLock", r"Atomic", r"DashMap", r"Arc<", r"\bstatic\b", r"lock\(\)", r"fs::", r"Path"],
    ]),
    Q::noul(
        "concurrency.atomics",
        D::Concurrency,
        "Does the changed code in `code` use atomics incorrectly, such as `Ordering::Relaxed` where the atomic value publishes other data, or a separate `load` followed by `store` where one atomic read-modify-write is needed?",
        "An atomic ordering is too weak for how the value is used, or a compound update is not atomic.",
        "Orderings match how the values are used and every compound update is a single atomic operation.",
    )
    .gate(&[&[r"Atomic", r"Ordering::"]]),
    Q::noul(
        "concurrency.unsafe_send_sync",
        D::Concurrency,
        "Does a changed `unsafe impl Send` or `unsafe impl Sync` in `code` apply to a type that contains data that is not safe to send or share across threads, such as `Rc`, `Cell`, `RefCell`, or an unsynchronised raw pointer?",
        "The type holds data that is unsafe to send or share across threads.",
        "Every field is safe to send or share, or access is synchronised.",
    )
    .gate(&[&[r"unsafe\s+impl[^{]*\b(Send|Sync)\b"]]),
    Q::noul(
        "concurrency.lock_scope",
        D::Concurrency,
        "Does the changed code in `code` keep a lock held while doing slow or re-entrant work that does not need the lock, such as I/O, acquiring another lock, or calling a callback?",
        "A lock is held across slow work or another lock acquisition that does not need it.",
        "Critical sections are short and only cover the data they protect.",
    )
    .gate(&[&[r"lock\(\)", r"\.read\(\)", r"\.write\(\)"], &[r"Mutex", r"RwLock"]]),
    // ---- unsafe ------------------------------------------------------------
    Q::noul(
        "unsafe.undefined_behaviour",
        D::Unsafe,
        "Can some input or call sequence make the changed `unsafe` code in `code` cause undefined behaviour, such as reading out of bounds, using a dangling pointer, creating two mutable references to the same data, reading uninitialised memory, or a `transmute` between incompatible types?",
        "A concrete input or call sequence breaks a safety requirement of an `unsafe` operation.",
        "Every safety requirement is upheld for all inputs, for example because the code checks bounds or lengths before the `unsafe` operation.",
    )
    .gate(&[&[r"\bunsafe\b"]]),
    Q::noul(
        "unsafe.missing_safety_comment",
        D::Unsafe,
        "Is a changed `unsafe` block or `unsafe fn` in `code` missing a comment, such as `// SAFETY:` or a `# Safety` doc section, that explains why it is sound?",
        "At least one changed `unsafe` block or function has no safety explanation.",
        "Every changed `unsafe` block or function has a safety explanation.",
    )
    .gate(&[&[r"\bunsafe\b"]])
    .threshold(0.5),
    // ---- ffi ----------------------------------------------------------------
    Q::noul(
        "ffi.boundary",
        D::Ffi,
        "Does the changed FFI code in `code` mishandle the `extern` boundary, for example by freeing memory with the wrong allocator, passing a string without a NUL terminator, not checking a pointer for null, or letting a panic unwind out of an `extern \"C\"` function?",
        "Ownership, nullability, string termination, or unwinding is handled wrongly at the boundary.",
        "The boundary handles ownership, null pointers, strings, and panics correctly.",
    )
    .gate(&[&[r#"extern\s+"C""#, r"no_mangle", r"repr\(C\)", r"CString", r"CStr", r"\*mut\s", r"\*const\s", r"c_char", r"c_void"]]),
    // ---- performance -----------------------------------------------------------
    Q::noul(
        "performance.repeated_work",
        D::Performance,
        "Does the changed code in `code` repeat avoidable work inside a loop, such as allocating, cloning, parsing, compiling a regex, or searching a list linearly, in a way that grows costly as the input grows?",
        "Work inside a loop could be hoisted or replaced with a lookup, and the cost grows with input size.",
        "The loop does no avoidable work, or the input is small and bounded.",
    )
    .gate(&[&[r"\bfor\b", r"\bwhile\b", r"\bloop\b", r"\.iter\(", r"\.map\(", r"\.contains\(", r"\.find\(", r"\.position\("]])
    .skip(&[Role::Test, Role::Example, Role::Bench, Role::BuildScript]),
    // ---- idiom / maintainability ---------------------------------------------
    QuestionSpec {
        id: "idiom.clarity",
        dimension: D::Idiom,
        instructions: "How clearly do the changed lines in `code` express their intent to an experienced Rust developer?",
        primitive: Primitive::Score {
            levels: &[
                "Clear: the intent is obvious and the code uses the language naturally.",
                "Mostly clear: some awkwardness, but the intent is still easy to follow.",
                "Unclear: control flow, naming, or structure hides what the code does.",
            ],
            bad_from: 2,
        },
        gate: &[],
        skip_roles: TESTS,
        unit: UnitKind::RustCode,
        threshold: None,
    },
    // ---- api -------------------------------------------------------------------
    Q::noul(
        "api.breaking_change",
        D::Api,
        "Does the change in `code` remove or alter a public (`pub`) item in a way that breaks existing callers, such as removing or renaming it, changing a function signature, adding trait bounds, adding a required trait method, or changing public fields?",
        "Code that compiled against the old public item would fail to compile or behave differently.",
        "No public item is removed or changed incompatibly; only new items are added or private code changed.",
    )
    .gate(&[&[r"\bpub\b"]])
    .skip(NON_LIB),
    // ---- macros ----------------------------------------------------------------
    Q::noul(
        "macros.hygiene",
        D::Macros,
        "Does a changed macro in `code` evaluate an argument expression more than once, or generate item or variable names that can collide with names at the call site?",
        "A macro argument is expanded more than once, or generated names can clash with the caller's names.",
        "Each argument is evaluated once and generated names cannot clash.",
    )
    .gate(&[&[r"macro_rules!", r"proc_macro", r"quote!"]]),
    // ---- serde -------------------------------------------------------------------
    Q::noul(
        "serde.compatibility",
        D::Serde,
        "Does the change in `code` alter how a type is serialized or deserialized, such as renaming a field, changing a field's type, adding `#[serde(untagged)]`, or adding `#[serde(default)]`, in a way that breaks previously stored or transmitted data or silently accepts invalid input?",
        "Existing serialized data no longer round-trips, or invalid input is now accepted silently.",
        "The serialized format stays compatible and invalid input is still rejected.",
    )
    .gate(&[&[r"Serialize", r"Deserialize", r"serde\("]]),
    // ---- security ----------------------------------------------------------------
    Q::noul(
        "security.injection",
        D::Security,
        "Does the changed code in `code` build a file path, shell command, SQL query, or URL from external input without validating or escaping it?",
        "External input reaches a path, command, query, or URL without validation or escaping.",
        "Inputs are validated or escaped, or they do not come from outside the program.",
    )
    .gate(&[&[r"Command::new", r"\.arg\(", r"query", r"execute", r"(?i)select\s", r"Path", r"\.join\(", r"File::", r"fs::", r"Url", r"format!"]])
    .skip(TESTS),
    Q::noul(
        "security.secret_exposure",
        D::Security,
        "Does the changed code in `code` write a secret, such as a password, token, API key, or session cookie, into a log, an error message, or `Debug` output?",
        "A secret value can end up in logs, error messages, or debug output.",
        "Secrets are never written to logs, errors, or debug output.",
    )
    .gate(&[
        &[r"log::", r"tracing", r"info!", r"debug!", r"warn!", r"error!", r"trace!", r"println!", r"eprintln!", r"format!", r"Debug"],
        &[r"(?i)password|passwd|secret|token|api_?key|credential|cookie|session|bearer"],
    ])
    .skip(TESTS),
    Q::noul(
        "security.tls_or_randomness",
        D::Security,
        "Does the changed code in `code` disable TLS certificate or hostname verification, or use a non-cryptographic random number generator to create secrets, tokens, or keys?",
        "TLS verification is disabled, or security-sensitive values come from a non-cryptographic generator.",
        "TLS verification stays on and security-sensitive randomness comes from a cryptographic source.",
    )
    .gate(&[&[r"danger", r"accept_invalid", r"(?i)verify_?none", r"NoVerifier", r"thread_rng", r"rand::", r"random\(", r"SmallRng", r"fastrand"]])
    .skip(TESTS),
    Q::noul(
        "security.unbounded_input",
        D::Security,
        "Does the changed code in `code` read or deserialize untrusted input without a size limit, so that a large or crafted input can exhaust memory?",
        "Untrusted input is read or parsed with no bound on its size.",
        "Input size is bounded, or the input is trusted.",
    )
    .gate(&[&[r"from_slice", r"from_str", r"from_reader", r"deserialize", r"read_to_end", r"read_to_string", r"with_capacity", r"bincode"]])
    .skip(TESTS),
    // ---- cargo (manifest units) ------------------------------------------------------
    Q::noul(
        "cargo.manifest_risk",
        D::Cargo,
        "Does this change to `Cargo.toml` in `code` alter dependencies, features, or build settings in a way that could break builds or change behaviour for existing users, for example removing a feature, changing default features, or moving a dependency to a new major version?",
        "The manifest change can break downstream builds or change behaviour.",
        "The manifest change is additive or internal and cannot affect existing users.",
    )
    .manifest(),
];

pub static PROFILES: &[Profile] = &[
    Profile {
        name: "tokio",
        detect_crates: &["tokio"],
        reference: "references/frameworks/tokio.md",
        questions: &[
            Q::noul(
                "tokio.runtime_nesting",
                D::Async,
                "Does the changed code in `code` call `block_on` or create a Tokio runtime from code that may already run inside a Tokio runtime, or call `block_in_place`, which panics on a `current_thread` runtime?",
                "A runtime is created or blocked on from inside async code, or `block_in_place` can run on a current-thread runtime.",
                "Runtimes are only created or blocked on from synchronous entry points.",
            )
            .gate(&[&[r"block_on", r"block_in_place", r"Runtime::new", r"Builder::new_"]]),
            Q::noul(
                "tokio.spawn_blocking_misuse",
                D::Async,
                "Does the changed code in `code` use `spawn_blocking` for work that runs forever, or rely on aborting a `spawn_blocking` task, even though blocking tasks cannot be aborted once started?",
                "`spawn_blocking` is used for endless work or is expected to be cancellable.",
                "`spawn_blocking` is used for finite blocking work only.",
            )
            .gate(&[&[r"spawn_blocking"]]),
            Q::noul(
                "tokio.no_shutdown",
                D::Async,
                "Does the changed code in `code` start a long-running task or loop that has no way to be told to stop, such as a `CancellationToken`, a shutdown channel, or a `JoinSet` that is dropped on shutdown?",
                "A long-running task or loop has no stop signal.",
                "Every long-running task can be stopped, or the task ends on its own.",
            )
            .gate(&[&[r"spawn", r"\bloop\b", r"interval"]])
            .skip(TESTS),
            Q::noul(
                "tokio.select_not_cancel_safe",
                D::Async,
                "Does a `tokio::select!` branch in `code` await `read_exact`, `read_to_end`, `read_to_string`, `write_all`, `Mutex::lock`, `Semaphore::acquire`, or another future that is not cancellation safe, inside a loop where losing the race drops progress?",
                "A non-cancellation-safe future is raced in a loop, so progress can be lost.",
                "Every raced future is cancellation safe, or it is pinned outside the loop and reused.",
            )
            .gate(&[&[r"select!"]]),
            Q::noul(
                "tokio.blocking_in_async",
                D::Async,
                "Does the changed code in `code` call `blocking_send`, `blocking_recv`, or `blocking_lock` from inside async code, where these methods panic?",
                "A `blocking_*` method is called from async code.",
                "`blocking_*` methods are only called from synchronous code.",
            )
            .gate(&[&[r"blocking_send", r"blocking_recv", r"blocking_lock", r"blocking_read", r"blocking_write"]]),
            Q::noul(
                "tokio.async_mutex_unneeded",
                D::Performance,
                "Does the changed code in `code` use `tokio::sync::Mutex` for data that is locked only briefly and never held across an `.await`, where `std::sync::Mutex` would be simpler and faster?",
                "An async mutex guards data that is never locked across an `.await`.",
                "The async mutex is held across `.await`, or a std mutex is already used.",
            )
            .gate(&[&[r"tokio::sync::(Mutex|RwLock)", r"sync::Mutex", r"Mutex<"]])
            .threshold(0.6),
        ],
    },
    Profile {
        name: "axum",
        detect_crates: &["axum"],
        reference: "references/frameworks/axum.md",
        questions: &[
            Q::noul(
                "axum.error_exposure",
                D::Security,
                "Does a changed Axum handler or error type in `code` send internal error details, such as database or I/O error messages, to the client, or return a success status code when the operation failed?",
                "Internal error details reach the HTTP response, or a failure is reported with a success status.",
                "Errors map to appropriate status codes with messages safe for clients.",
            )
            .gate(&[&[r"IntoResponse", r"StatusCode", r"Response", r"Json\("]]),
            Q::noul(
                "axum.layer_order",
                D::Correctness,
                "Does the change in `code` add or reorder middleware so that authentication, rate limiting, timeouts, or CORS run in the wrong order, given that with `Router::layer` the layer added last runs first on the request and with `ServiceBuilder` layers run top to bottom?",
                "Middleware runs in an order that defeats its purpose.",
                "Middleware runs in an order consistent with its purpose.",
            )
            .gate(&[&[r"\.layer\(", r"route_layer", r"ServiceBuilder"]]),
            Q::noul(
                "axum.extension_state",
                D::Correctness,
                "Does the changed code in `code` use `Extension` to pass application state that may be missing for some routes, which fails at runtime with a 500 error instead of at compile time like `State`?",
                "`Extension` carries state that some routes may not have.",
                "State is passed with `State`, or the `Extension` is always present.",
            )
            .gate(&[&[r"Extension"]]),
            Q::noul(
                "axum.blocking_handler",
                D::Async,
                "Does a changed Axum handler in `code` do blocking or CPU-heavy work, such as synchronous database calls, file I/O, or password hashing, directly in the async handler?",
                "The handler blocks the executor thread.",
                "Blocking work is offloaded or absent.",
            )
            .gate(&[&[r"async\s+fn"], &[r"std::fs", r"fs::", r"hash", r"bcrypt", r"argon2", r"diesel", r"rusqlite", r"thread::sleep", r"blocking"]]),
        ],
    },
    Profile {
        name: "dioxus",
        detect_crates: &["dioxus"],
        reference: "references/frameworks/dioxus.md",
        questions: &[
            Q::noul(
                "dioxus.guard_across_await",
                D::Async,
                "In `code`, is a guard returned by a signal's `.read()` or `.write()` still alive at an `.await` point?",
                "A signal read or write guard is held while the code awaits.",
                "Signal guards are dropped before every `.await`.",
            )
            .gate(&[&[r"\.await"], &[r"\.read\(\)", r"\.write\(\)", r"with_mut"]]),
            Q::noul(
                "dioxus.read_write_overlap",
                D::Correctness,
                "In `code`, can a signal's `.read()` guard still be alive when the same signal is written, for example writing to a signal inside a loop over its own `.read()` value, which panics at runtime with an already-borrowed error?",
                "A read guard and a write to the same signal overlap.",
                "Reads and writes of the same signal never overlap.",
            )
            .gate(&[&[r"\.read\(\)", r"\.with\(", r"\.iter\(\)"], &[r"\.write\(\)", r"\.set\(", r"with_mut", r"\+=", r"\.push\("]]),
            Q::noul(
                "dioxus.effect_loop",
                D::Correctness,
                "Does a changed `use_effect` or `use_memo` in `code` write to a signal that it also reads without `peek()`, so that it can keep re-running itself?",
                "The effect or memo writes a signal it subscribes to.",
                "The effect or memo never writes a signal it subscribes to.",
            )
            .gate(&[&[r"use_effect", r"use_memo"]]),
            Q::noul(
                "dioxus.hook_rules",
                D::Correctness,
                "Does the changed code in `code` call a hook (a function named `use_...`) conditionally, inside a loop, inside a closure or event handler, or after an early return, so the order of hook calls can change between renders?",
                "A hook call can be skipped or reordered between renders.",
                "Hooks are called unconditionally at the top level of the component, in the same order every render.",
            )
            .gate(&[&[r"\buse_[a-z_]+\("]]),
            Q::noul(
                "dioxus.stale_capture",
                D::Correctness,
                "Does a changed closure passed to `use_effect`, `use_memo`, `use_resource`, or `use_future` in `code` use a plain prop or local value (not a signal) that can change between renders, so the closure keeps using a stale value?",
                "The closure captures a changing non-signal value that is not tracked, for example without `use_reactive`.",
                "The closure only uses signals or values that never change.",
            )
            .gate(&[&[r"use_effect", r"use_memo", r"use_resource", r"use_future"]]),
            Q::noul(
                "dioxus.server_fn_trust",
                D::Security,
                "Does a changed server function in `code` trust its arguments without validating them or checking that the caller is authorised, even though server functions are public HTTP endpoints?",
                "The server function acts on unvalidated input or without an authorisation check.",
                "The server function validates input and checks authorisation, or it only returns public data.",
            )
            .gate(&[&[r"#\[server", r"#\[get", r"#\[post", r"#\[put", r"#\[delete", r"#\[patch"]]),
            Q::noul(
                "dioxus.untracked_dependency",
                D::Correctness,
                "In `code`, does a `use_server_future` closure read a signal only inside its `async` block, where the read is not tracked, so changes to that signal do not re-run it?",
                "A signal is read only inside the async block of `use_server_future`.",
                "Signals are read in the closure before the async block, or none are read.",
            )
            .gate(&[&[r"use_server_future"]]),
        ],
    },
];

// ---- reference facts ------------------------------------------------------------

/// Short documented facts added to the state when their gate matches the
/// code. The Models page recommends putting reference material in `state`
/// rather than expecting the model to recall it. Each fact is taken from
/// the official documentation (see DESIGN.md) and must stay literal.
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
    (
        r"use_signal|Signal<|\.read\(\)",
        "In Dioxus 0.7, holding a signal's `.read()` guard while writing the same signal panics with an already-borrowed error, and signal guards must not be held across `.await`.",
    ),
];

/// Facts whose gate matches `text`, at most five.
pub fn facts_for(text: &str) -> Vec<&'static str> {
    use std::sync::LazyLock;
    static COMPILED: LazyLock<Vec<(regex::Regex, &'static str)>> = LazyLock::new(|| {
        FACTS
            .iter()
            .map(|(pat, fact)| {
                (
                    regex::Regex::new(pat).expect("fact gates are tested"),
                    *fact,
                )
            })
            .collect()
    });
    COMPILED
        .iter()
        .filter(|(re, _)| re.is_match(text))
        .map(|(_, f)| *f)
        .take(5)
        .collect()
}

// ---- verification questions (precision stage) --------------------------------

pub const VERIFY_SUPPORTED: &str = "supported";
pub const VERIFY_SEVERITY: &str = "severity";
pub const VERIFY_CATEGORY: &str = "category";

pub const SEVERITY_OPTIONS: &[(&str, &str)] = &[
    (
        "critical",
        "Undefined behaviour, memory unsafety, a deadlock, data loss or corruption, or a security vulnerability reachable in normal use.",
    ),
    (
        "high",
        "Wrong results, a crash or panic, or a resource leak in a realistic situation.",
    ),
    (
        "medium",
        "A defect that needs unusual conditions, noticeably degrades performance, or makes failures hard to diagnose.",
    ),
    (
        "low",
        "A minor issue with little practical impact, such as a missing comment or a small inefficiency.",
    ),
];

pub const CATEGORY_OPTIONS: &[(&str, &str)] = &[
    (
        "real_defect",
        "The code is wrong: it can misbehave, crash, leak, be unsound, or be insecure as the claim describes.",
    ),
    (
        "debatable_tradeoff",
        "The code works; the claim describes a reasonable design or performance trade-off that people could disagree on.",
    ),
    (
        "style_preference",
        "The claim is about naming, formatting, or code style rather than behaviour.",
    ),
    ("not_supported", "The code does not do what the claim says."),
];

pub fn verify_questions() -> serde_json::Value {
    let choice = |instructions: &str, opts: &[(&str, &str)]| {
        let mut criteria = serde_json::Map::new();
        for (k, v) in opts {
            criteria.insert((*k).into(), serde_json::Value::String((*v).into()));
        }
        serde_json::json!({"type": "choice", "instructions": instructions, "criteria": criteria})
    };
    serde_json::json!({
        VERIFY_SUPPORTED: {
            "type": "noul",
            "instructions": {
                "question": "Is the `claim` true of the code in `code`?",
                "focus": "Judge only whether the defect described in `claim` is actually present in the lines marked `>` and the code they depend on."
            },
            "criteria": {
                "true": "The code contains the defect exactly as `claim` describes it.",
                "false": "The defect is absent, the claim misreads the code, or `code` does not contain enough to confirm it."
            }
        },
        VERIFY_SEVERITY: choice("If the `claim` is true, how severe is the defect it describes?", SEVERITY_OPTIONS),
        VERIFY_CATEGORY: choice("Which option best describes the `claim` about `code`?", CATEGORY_OPTIONS),
    })
}

/// Serialize one question spec to the API's question shape.
pub fn to_api(q: &QuestionSpec) -> serde_json::Value {
    match q.primitive {
        Primitive::Noul { yes, no } => serde_json::json!({
            "type": "noul",
            "instructions": q.instructions,
            "criteria": {"true": yes, "false": no},
        }),
        Primitive::Score { levels, .. } => serde_json::json!({
            "type": "score",
            "instructions": q.instructions,
            "criteria": levels,
        }),
        Primitive::Choice { options, .. } => {
            let mut criteria = serde_json::Map::new();
            for (k, v) in options {
                criteria.insert((*k).into(), serde_json::Value::String((*v).into()));
            }
            serde_json::json!({"type": "choice", "instructions": q.instructions, "criteria": criteria})
        }
    }
}

/// All specs, core first, then every profile's.
pub fn all_specs() -> impl Iterator<Item = (&'static QuestionSpec, Option<&'static str>)> {
    CORE.iter().map(|q| (q, None)).chain(
        PROFILES
            .iter()
            .flat_map(|p| p.questions.iter().map(move |q| (q, Some(p.name)))),
    )
}

pub fn reference_for(d: Dimension) -> String {
    format!("references/dimensions/{}.md", d.name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_unique_and_prefixed() {
        let mut seen = HashSet::new();
        for (q, profile) in all_specs() {
            assert!(seen.insert(q.id), "duplicate id {}", q.id);
            let prefix = q.id.split('.').next().unwrap();
            match profile {
                Some(p) => assert_eq!(prefix, p, "{}", q.id),
                None => assert_eq!(prefix, q.dimension.name(), "{}", q.id),
            }
        }
    }

    #[test]
    fn gates_compile() {
        for (q, _) in all_specs() {
            for group in q.gate {
                for pat in *group {
                    regex::Regex::new(pat).unwrap_or_else(|e| panic!("{}: {e}", q.id));
                }
            }
        }
    }

    #[test]
    fn instructions_follow_jev_guidance() {
        for (q, _) in all_specs() {
            let i = q.instructions.to_lowercase();
            // Name the state field, avoid negated framings and asks that
            // belong in code.
            if q.unit == UnitKind::RustCode {
                assert!(i.contains("`code`"), "{} must reference `code`", q.id);
            }
            for banned in ["unless", "how many", "line number", "compile?", "not un"] {
                assert!(!i.contains(banned), "{} contains {banned:?}", q.id);
            }
            assert!(q.instructions.len() < 420, "{} is too long", q.id);
        }
    }

    #[test]
    fn facts_gate_on_code() {
        assert!(facts_for("fn f() {}").is_empty());
        let f = facts_for("tokio::select! { x = s.read_exact(&mut b) => {} }");
        assert!(f.iter().any(|t| t.contains("cancellation safe")));
        assert!(facts_for("let n = x as u16;")[0].contains("truncates"));
    }

    #[test]
    fn dimension_parse_roundtrip() {
        for d in Dimension::ALL {
            assert_eq!(Dimension::parse(d.name()), Some(d));
        }
        assert_eq!(Dimension::parse("errors"), Some(Dimension::ErrorHandling));
        assert_eq!(Dimension::parse("nope"), None);
    }

    #[test]
    fn triage_bars_lower_than_report_bars() {
        for d in Dimension::ALL {
            assert!(d.default_triage_threshold() < d.default_report_threshold());
        }
    }
}
