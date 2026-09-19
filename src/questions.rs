//! Every Jev question and every default threshold lives in this file, so the
//! part of the system that most needs human review can be read in one place.
//!
//! Design rules, from the Jev 1.13 jaggedness page:
//! - literal, single-hop instructions that name the state field (`code`);
//! - one defect per question: fan-out is cheap, and a specific id gives the
//!   reviewer a specific place to look;
//! - Noul criteria mirror the instruction (`true` = the defect is present);
//! - no counting, arithmetic, line numbers, or anything a tool can answer
//!   (clippy's `undocumented_unsafe_blocks` and `missing_safety_doc` own
//!   "is this `unsafe` documented", so there is no question for it);
//! - lexical gates in code decide which questions apply to a unit;
//! - every flag rule reads exactly one answer (no consistency assumptions).
//!
//! Gate contract: gates run on the unit text as it is sent to Jev, including
//! the diff marker in the first column (`+` added, `-` removed, space for
//! context; see `context::render`). Verification excerpts prepend a claim
//! column (`>` or space) to that marker; question gates never see them, but
//! the fact gates in `facts.rs` do, so both follow the same rule. Every
//! pattern is written so that a marker alone can never open a gate, and the
//! `gates_stay_closed_on_plain_code` and `diff_markers_alone_open_nothing`
//! tests enforce that.

use crate::rust_project::Role;
use regex::RegexSet;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Defines the enum, `ALL`, `name()` and the serde names from one list, so
/// they cannot drift apart when a dimension is added.
macro_rules! dimensions {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        pub enum Dimension {
            $(#[serde(rename = $name)] $variant,)+
        }

        impl Dimension {
            pub const ALL: &'static [Dimension] = &[$(Dimension::$variant,)+];

            pub fn name(self) -> &'static str {
                match self {
                    $(Dimension::$variant => $name,)+
                }
            }
        }
    };
}

dimensions! {
    Correctness => "correctness",
    Ownership => "ownership",
    TypeDesign => "type_design",
    ErrorHandling => "error_handling",
    Async => "async",
    Concurrency => "concurrency",
    Unsafe => "unsafe",
    Ffi => "ffi",
    Performance => "performance",
    Idiom => "idiom",
    Api => "api",
    Macros => "macros",
    Serde => "serde",
    Security => "security",
    Testing => "testing",
    Cargo => "cargo",
}

impl Dimension {
    /// Accepts the canonical name plus the aliases people actually type.
    pub fn parse(s: &str) -> Option<Dimension> {
        let normalised = s.trim().to_ascii_lowercase().replace('-', "_");
        let canonical = match normalised.as_str() {
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
        };
        Dimension::ALL
            .iter()
            .copied()
            .find(|d| d.name() == canonical)
    }

    /// Triage bar on a question's own answer: low where a miss is expensive
    /// (recall-oriented).
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

    /// Report bar on the verification answer (`P(supported)`): high, because
    /// a false positive reaching the user is the expensive error
    /// (precision-oriented). This is a bar on a different question from the
    /// triage bar, so the two numbers are not comparable with each other.
    pub fn default_report_threshold(self) -> f64 {
        match self {
            Dimension::Unsafe | Dimension::Idiom | Dimension::TypeDesign => 0.80,
            _ => 0.70,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    /// Flags when the probability of "yes" reaches the threshold.
    Noul { yes: &'static str, no: &'static str },
    /// Levels run from the low end of the scale to the high end, as the API
    /// requires, and are indexed from 0. Flags when the probability mass on
    /// levels `bad_from..` reaches the threshold.
    Score {
        levels: &'static [&'static str],
        bad_from: usize,
    },
}

impl Primitive {
    pub fn name(&self) -> &'static str {
        match self {
            Primitive::Noul { .. } => "noul",
            Primitive::Score { .. } => "score",
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
    /// text for the question to be asked. Empty = always asked. Use
    /// [`QuestionSpec::applies`] rather than compiling these yourself.
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
    const fn score(
        id: &'static str,
        dimension: Dimension,
        instructions: &'static str,
        levels: &'static [&'static str],
        bad_from: usize,
    ) -> QuestionSpec {
        QuestionSpec {
            id,
            dimension,
            instructions,
            primitive: Primitive::Score { levels, bad_from },
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

    pub fn triage_threshold(&self) -> f64 {
        self.threshold
            .unwrap_or_else(|| self.dimension.default_triage_threshold())
    }

    /// Whether this question should be asked about a unit. Gates are compiled
    /// once for the whole process, not once per unit.
    pub fn applies(&self, unit: UnitKind, role: Role, text: &str) -> bool {
        self.unit == unit && !self.skip_roles.contains(&role) && self.gate_open(text)
    }

    pub fn gate_open(&self, text: &str) -> bool {
        GATES
            .get(self.id)
            .is_none_or(|groups| groups.iter().all(|set| set.is_match(text)))
    }
}

static GATES: LazyLock<HashMap<&'static str, Vec<RegexSet>>> = LazyLock::new(|| {
    all_specs()
        .map(|(q, _)| {
            let groups = q
                .gate
                .iter()
                .map(|group| {
                    RegexSet::new(group.iter())
                        .expect("gate patterns are compiled by the `gates_compile` test")
                })
                .collect();
            (q.id, groups)
        })
        .collect()
});

/// A framework profile is data: detection crates, extra questions, the
/// reference file the reviewer loads when the profile is active, and the
/// documentation its embedded facts were checked against.
#[derive(Debug)]
pub struct Profile {
    pub name: &'static str,
    pub detect_crates: &'static [&'static str],
    pub questions: &'static [QuestionSpec],
    pub reference: &'static str,
    /// Instructions below state framework behaviour as fact. A stale fact
    /// makes Jev flag correct code with confidence, so record the source and
    /// re-check it when the framework's major version moves.
    pub verified_against: &'static str,
}

const NON_PROD: &[Role] = &[Role::Test, Role::Example, Role::Bench];
const NON_PROD_OR_BUILD: &[Role] = &[Role::Test, Role::Example, Role::Bench, Role::BuildScript];
const NON_LIB: &[Role] = &[
    Role::Test,
    Role::Example,
    Role::Bench,
    Role::BuildScript,
    Role::Binary,
];
const TESTS: &[Role] = &[Role::Test];

// Gate fragments shared by several questions.
/// `a[i]`, `f()[0]`, `m[k][j]`; not `#[attr]`, `vec![..]`, `[u8; 4]`, `&[T]`.
const INDEXING: &str = r"[\w)\]]\[";
/// A binary `+ - *` (or the compound-assign form) between two operands. A
/// diff marker has no operand before it on its line (hence `[ \t]`, not
/// `\s`, which would reach back across the line break), and `->` has none
/// after it.
const ARITHMETIC: &str = r"[\w)\]][ \t]*[-+*]=?[ \t]*[\w(]";
const LOOP: &str = r"\b(for|while|loop)\b|\.for_each\(|\.map\(|\.filter\(|\.fold\(";
const LOCK_CALL: &str = r"\.lock\(\)|\.read\(\)|\.write\(\)";

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
    .gate(&[&[INDEXING, r"split_at|get_unchecked|\.windows\(|\.chunks", r"len\(\)\s*[-+]\s*\d"]])
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
        &[ARITHMETIC, r"\w\s*<<\s*\w"],
    ])
    .skip(NON_PROD),
    Q::noul(
        "correctness.wildcard",
        D::Correctness,
        "Does a changed `match` in `code` use a wildcard `_` arm on an enum where a variant added later would be handled wrongly by that arm with no warning?",
        "The wildcard arm would silently apply behaviour that is wrong for new variants.",
        "The wildcard arm is correct for any future variant, or the match is not over an enum.",
    )
    .gate(&[&[r"\bmatch\b"], &[r"\b_\s*=>"]])
    .skip(NON_PROD)
    .threshold(0.45),
    Q::noul(
        "correctness.drop_order",
        D::Correctness,
        "Does the changed code in `code` drop a value earlier or later than the code relies on, for example a guard bound to `let _ =` that is dropped immediately, or a temporary that lives to the end of a statement?",
        "A value's drop point differs from what the surrounding code relies on.",
        "Every value lives exactly as long as the code relies on.",
    )
    .gate(&[&[r"let\s+_\s*=", r"\bdrop\(", r"impl\s+Drop", r"_guard", r"mem::forget", r"\.lock\(\)"]])
    .skip(NON_PROD),
    // ---- ownership ---------------------------------------------------
    Q::noul(
        "ownership.clone",
        D::Ownership,
        "Do the changed lines in `code` copy data that could have been borrowed instead, where the copy is large or runs inside a loop?",
        "An avoidable copy of a large value, or a copy repeated inside a loop.",
        "Every copy is needed, or it is cheap: an `Arc` or `Rc` clone, a small value, or data moved into a spawned task or thread.",
    )
    .gate(&[&[r"\.clone\(\)", r"\.to_owned\(\)", r"\.to_vec\(\)", r"\.to_string\(\)", r"String::from", r"\.cloned\(\)"]])
    .skip(NON_PROD),
    Q::noul(
        "ownership.signature",
        D::Ownership,
        "Does a changed function signature in `code` take ownership of a `String`, `Vec`, `PathBuf`, or other owned value that the function only reads?",
        "The function only reads the owned argument, so callers must allocate or give up their value for no reason.",
        "The function stores, moves, or mutates the owned argument, or it takes a borrowed type already.",
    )
    .gate(&[&[r"fn\s+\w+[^{;]*:\s*(String|Vec<|PathBuf|Box<)"]])
    .skip(NON_PROD),
    // ---- type design -------------------------------------------------
    // Asked wherever types are defined, in applications as well as libraries.
    Q::noul(
        "type_design.loose_types",
        D::TypeDesign,
        "Does a changed struct, enum, or function signature in `code` represent a fixed set of states or modes with booleans, strings, integers, or sentinel values where an enum or a dedicated type would prevent invalid values?",
        "Invalid combinations or values are representable and a simple enum or newtype would rule them out.",
        "The representation is appropriate, or a dedicated type would add more complexity than it removes.",
    )
    .gate(&[&[
        r"\bstruct\s+\w+",
        r"fn\s+\w+[^{;]*:\s*(bool|&str|String|u8|i32|u32)\b",
        r"==\s*-1\b",
    ]])
    .skip(NON_PROD_OR_BUILD),
    // ---- error handling ------------------------------------------------
    Q::noul(
        "error_handling.swallowed",
        D::ErrorHandling,
        "Do the changed lines in `code` discard an error so that a failure goes unnoticed, for example `let _ =` on a `Result`, `.ok()`, `unwrap_or_default()`, or an `Err(_)` branch that does nothing?",
        "A failure is silently ignored and the program continues as if the operation succeeded.",
        "Every error is handled, returned, logged, or deliberately ignored where ignoring it is correct.",
    )
    .gate(&[&[r"let\s+_\s*=", r"\.ok\(\)", r"unwrap_or_default", r"unwrap_or\(", r"Err\(_\)", r"if\s+let\s+Ok", r"\.is_ok\(\)"]])
    .skip(TESTS),
    Q::noul(
        "error_handling.panic",
        D::ErrorHandling,
        "Can the changed lines in `code` panic on input or state that the program does not control, for example through `unwrap`, `expect`, indexing, `panic!`, or `unreachable!`?",
        "A panic is reachable from external input, I/O results, or other state the program does not control.",
        "Every possible panic enforces an invariant that the surrounding code has already checked or documented.",
    )
    .gate(&[&[r"\.unwrap\(\)", r"\.expect\(", r"panic!", r"unreachable!", r"todo!", r"unimplemented!", INDEXING]])
    .skip(NON_PROD),
    Q::noul(
        "error_handling.lossy",
        D::ErrorHandling,
        "Does the changed code in `code` convert or replace an error in a way that drops the original error's message, source, or context?",
        "The original error's information is discarded, so the caller cannot tell what actually failed.",
        "The original error is kept, wrapped, or chained, or it carries no useful information.",
    )
    .gate(&[&[r"map_err", r"\.ok_or", r"Box<dyn\s+(std::error::)?Error", r"anyhow!|bail!", r"impl\s+From<", r#"Err\(\s*(format!|String::|")"#]])
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
    .gate(&[&[r"\.await"], &[LOCK_CALL, r"\.borrow(_mut)?\(\)", r"Mutex", r"RwLock"]])
    .skip(TESTS),
    Q::noul(
        "async.blocking_call",
        D::Async,
        "Does an `async` function or block in `code` call an operation that blocks the thread, such as `std::thread::sleep`, `std::fs` or `std::net` I/O, or a blocking HTTP client, directly instead of through `spawn_blocking`?",
        "Blocking work runs directly on the async executor thread.",
        "All blocking work is offloaded, or the operation is non-blocking or trivially short.",
    )
    .gate(&[&[r"\basync\b"], &[r"thread::sleep", r"std::fs|\bfs::", r"File::", r"std::net", r"blocking", r"read_to_string", r"\.join\(\)", r"Command::new", r"stdin", r"sync_channel", r"\.recv\(\)"]]),
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
    .gate(&[&[r"\.await"], &[r"\b(for|while|loop)\b"]])
    .skip(NON_PROD)
    .threshold(0.55),
    // ---- concurrency -----------------------------------------------------
    Q::noul(
        "concurrency.check_then_act",
        D::Concurrency,
        "Does the changed code in `code` check shared state and then act on it in a separate step, so that another thread or task can change the state in between?",
        "The check and the action are not atomic with respect to other threads or tasks.",
        "The check and the action happen under one lock or one atomic operation, or the state is not shared.",
    )
    .gate(&[
        &[LOCK_CALL, r"\.load\(", r"contains", r"\.get\(", r"exists", r"is_some", r"is_none"],
        &[r"Mutex", r"RwLock", r"Atomic", r"DashMap", r"Arc<", r"\bstatic\b", r"\bfs::", r"\bPath"],
    ]),
    Q::noul(
        "concurrency.atomics",
        D::Concurrency,
        "Does the changed code in `code` use atomics incorrectly, such as `Ordering::Relaxed` where the atomic value publishes other data, or a separate `load` followed by `store` where one atomic read-modify-write is needed?",
        "An atomic ordering is too weak for how the value is used, or a compound update is not atomic.",
        "Orderings match how the values are used and every compound update is a single atomic operation.",
    )
    .gate(&[&[r"Atomic[A-Z]\w+", r"Ordering::(Relaxed|SeqCst|Acquire|Release|AcqRel)"]]),
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
    .gate(&[&[LOCK_CALL], &[r"Mutex", r"RwLock"]]),
    // ---- unsafe ------------------------------------------------------------
    // Three narrow questions instead of one that lists five kinds of UB.
    Q::noul(
        "unsafe.memory_access",
        D::Unsafe,
        "Can some input or call sequence make the changed `unsafe` code in `code` read or write memory it must not touch: out of bounds, through a dangling or null pointer, or before the memory is initialised?",
        "A concrete input or call sequence makes an `unsafe` operation access invalid or uninitialised memory.",
        "Every `unsafe` memory access is valid for all inputs, for example because the code checks bounds or lengths first.",
    )
    .gate(&[&[r"\bunsafe\b"]]),
    Q::noul(
        "unsafe.aliasing",
        D::Unsafe,
        "Can the changed `unsafe` code in `code` create two live mutable references to the same data, or a mutable reference while a shared reference to the same data is still in use?",
        "Two references that Rust forbids from coexisting can be alive at the same time.",
        "References created in the `unsafe` code never alias in a forbidden way.",
    )
    .gate(&[&[r"\bunsafe\b"], &[r"&mut\b", r"\*mut\b", r"as_mut", r"from_raw", r"UnsafeCell", r"get_unchecked_mut"]]),
    Q::noul(
        "unsafe.transmute",
        D::Unsafe,
        "Does a changed `transmute` or pointer cast in `code` convert between types whose size, alignment, layout, or valid values differ?",
        "The source and target types are not layout compatible, or the source can hold a value that is invalid for the target.",
        "The two types have the same size, alignment, and layout, and every source value is valid for the target.",
    )
    .gate(&[&[r"transmute", r"\bas\s+\*(const|mut)\b", r"\.cast::<", r"from_raw_parts"]]),
    // ---- ffi ----------------------------------------------------------------
    Q::noul(
        "ffi.ownership",
        D::Ffi,
        "Does the changed FFI code in `code` free memory with a different allocator from the one that allocated it, free it twice, or leave it unclear which side of the `extern` boundary must free it?",
        "Memory crossing the boundary is freed by the wrong side, freed twice, or never freed.",
        "Each allocation crossing the boundary is freed exactly once by the side that allocated it.",
    )
    .gate(&[&[r"into_raw|from_raw", r"\bfree\(", r"Box::leak", r"mem::forget", r"CString"], &[r#"extern\s+"C""#, r"no_mangle", r"c_char", r"c_void", r"\*(mut|const)\s"]]),
    Q::noul(
        "ffi.pointers_and_strings",
        D::Ffi,
        "Does the changed FFI code in `code` dereference a pointer received from foreign code before checking it for null, or pass or read a string that lacks a NUL terminator or may contain invalid UTF-8?",
        "A foreign pointer is used before a null check, or a string crosses the boundary with the wrong termination or encoding.",
        "Foreign pointers are checked for null before use and strings are converted with `CStr` or `CString` correctly.",
    )
    .gate(&[&[r"c_char", r"CStr", r"CString", r"\*(mut|const)\s", r"c_void"], &[r#"extern\s+"C""#, r"no_mangle", r"\bunsafe\b"]]),
    Q::noul(
        "ffi.unwind",
        D::Ffi,
        "Can a panic unwind out of a changed `extern \"C\"` function in `code`, for example from `unwrap`, indexing, or an allocation, with nothing such as `catch_unwind` to stop it?",
        "A panic can cross the `extern \"C\"` boundary.",
        "The function body cannot panic, or panics are caught before the boundary.",
    )
    .gate(&[&[r#"extern\s+"C"\s+fn"#]]),
    // ---- performance -----------------------------------------------------------
    Q::noul(
        "performance.repeated_work",
        D::Performance,
        "Does the changed code in `code` repeat avoidable work inside a loop, such as allocating, cloning, parsing, compiling a regex, or searching a list linearly, in a way that grows costly as the input grows?",
        "Work inside a loop could be hoisted or replaced with a lookup, and the cost grows with input size.",
        "The loop does only necessary work, or the input is small and bounded.",
    )
    .gate(&[
        &[LOOP],
        &[r"\.clone\(\)|\.to_(string|owned|vec)\(\)", r"String::(from|new)|Vec::new|format!", r"Regex::new|\.parse\b|from_str", r"\.contains\(|\.find\(|\.position\(", r"\.collect"],
    ])
    .skip(NON_PROD_OR_BUILD),
    // ---- idiom / maintainability ---------------------------------------------
    // The scale runs the same way as the question: a higher level means
    // harder to follow.
    Q::score(
        "idiom.clarity",
        D::Idiom,
        "How hard is it for an experienced Rust developer to follow what the changed lines in `code` do?",
        &[
            "Easy: the intent is obvious and the code uses the language naturally.",
            "Some effort: there is awkwardness, but the intent can still be followed.",
            "Hard: control flow, naming, or structure hides what the code does.",
        ],
        2,
    )
    .skip(TESTS),
    // ---- api -------------------------------------------------------------------
    Q::noul(
        "api.breaking_change",
        D::Api,
        "Does the change in `code` remove or alter a public (`pub`) item in a way that breaks existing callers, such as removing or renaming it, changing a function signature, adding trait bounds, adding a required trait method, or changing public fields?",
        "Code written against the old public item would stop building or behave differently.",
        "Every public item that existed before still exists with a compatible signature; only new items are added or private code changed.",
    )
    .gate(&[&[r"\bpub\s+(fn|struct|enum|trait|type|const|static|mod|use|unsafe|async)\b", r"\bpub\s+\w+\s*:"]])
    .skip(NON_LIB),
    // ---- macros ----------------------------------------------------------------
    Q::noul(
        "macros.double_evaluation",
        D::Macros,
        "Does a changed macro in `code` expand one of its argument expressions more than once, so that an argument with side effects runs repeatedly?",
        "A macro argument appears more than once in the expansion.",
        "Each argument is bound to a local once and the local is reused.",
    )
    .gate(&[&[r"macro_rules!"]]),
    Q::noul(
        "macros.name_collision",
        D::Macros,
        "Does a changed macro in `code` generate items, or in a procedural macro variables, with fixed names that can collide with names at the call site?",
        "Generated names can clash with names the caller already uses.",
        "Generated names are hygienic, derived from the input, or unique.",
    )
    .gate(&[&[r"macro_rules!", r"proc_macro", r"quote!"]]),
    // ---- serde -------------------------------------------------------------------
    Q::noul(
        "serde.compatibility",
        D::Serde,
        "Does the change in `code` alter how a type is serialized, such as renaming a field, changing a field's type, or adding `#[serde(untagged)]`, so that previously stored or transmitted data no longer round-trips?",
        "Data written by the old version cannot be read by the new one, or the reverse.",
        "The serialized format stays compatible with data written before the change.",
    )
    .gate(&[&[r"Serialize", r"Deserialize", r"serde\("]]),
    Q::noul(
        "serde.silent_default",
        D::Serde,
        "Does a changed `#[serde(default)]`, `Option` field, or `#[serde(other)]` in `code` make deserialization accept input that is missing a required value or carries an unknown one, where that input should be rejected?",
        "Invalid or incomplete input is now accepted and filled with a default.",
        "Defaults apply only where a missing value is valid.",
    )
    .gate(&[&[r"serde\([^)]*(default|other|skip)"]]),
    // ---- security ----------------------------------------------------------------
    Q::noul(
        "security.injection",
        D::Security,
        "Does the changed code in `code` build a file path, shell command, SQL query, or URL from external input without validating or escaping it?",
        "External input reaches a path, command, query, or URL without validation or escaping.",
        "Inputs are validated or escaped, or they come from inside the program.",
    )
    .gate(&[&[
        r"Command::new|\.arg\(|\.args\(",
        r"sqlx::|\.query\(|\.execute\(|(?i)\b(select|insert|update|delete)\b.*\b(from|into|set)\b",
        r"Path(Buf)?::|\.join\(|File::(open|create)|\bfs::",
        r#"Url::parse|format!\(\s*"https?:"#,
    ]])
    .skip(TESTS),
    Q::noul(
        "security.secret_exposure",
        D::Security,
        "Does the changed code in `code` write a secret, such as a password, token, API key, or session cookie, into a log, an error message, or `Debug` output?",
        "A secret value can end up in logs, error messages, or debug output.",
        "Secrets stay out of logs, errors, and debug output.",
    )
    .gate(&[
        &[r"log::", r"tracing", r"info!", r"debug!", r"warn!", r"error!", r"trace!", r"println!", r"eprintln!", r"format!", r"Debug"],
        &[r"(?i)password|passwd|secret|token|api_?key|credential|cookie|session|bearer"],
    ])
    .skip(TESTS),
    Q::noul(
        "security.tls_verification",
        D::Security,
        "Does the changed code in `code` disable TLS certificate verification or hostname verification?",
        "Certificate or hostname verification is turned off.",
        "TLS verification stays on.",
    )
    .gate(&[&[r"danger", r"accept_invalid", r"(?i)verify_?none", r"(?i)no_?verif", r"set_verify"]])
    .skip(TESTS),
    Q::noul(
        "security.weak_randomness",
        D::Security,
        "Does the changed code in `code` create a secret, token, key, nonce, or password with a random number generator that is not cryptographically secure?",
        "A security-sensitive value comes from a non-cryptographic generator.",
        "Security-sensitive values come from a cryptographic source, or the random values are not security sensitive.",
    )
    .gate(&[&[r"thread_rng|\brand::|random\(|SmallRng|fastrand|StdRng::seed"]])
    .skip(TESTS),
    Q::noul(
        "security.unbounded_input",
        D::Security,
        "Does the changed code in `code` read or deserialize untrusted input without a size limit, so that a large or crafted input can exhaust memory?",
        "Untrusted input is read or parsed with no bound on its size.",
        "Input size is bounded, or the input is trusted.",
    )
    .gate(&[&[r"from_slice", r"from_reader", r"read_to_end", r"read_to_string", r"bincode", r"\.bytes\(\)\.await", r"to_bytes\("]])
    .skip(TESTS),
    // ---- testing -----------------------------------------------------------------
    // Whether the diff touches tests at all is a fact about the diff and is
    // computed in code. This asks the one local thing Jev can judge.
    Q::noul(
        "testing.weak_assertion",
        D::Testing,
        "Does a changed test function in `code` run the code under test without asserting anything about its result, or assert something that is always true?",
        "A changed test would still pass if the code under test returned a wrong result.",
        "Every changed test asserts on the behaviour it exercises, or expects a panic or an error explicitly.",
    )
    .gate(&[&[r"#\[(\w+::)*test\b", r"#\[rstest", r"proptest!"]]),
    // ---- cargo (manifest units) ------------------------------------------------------
    Q::noul(
        "cargo.manifest_risk",
        D::Cargo,
        "Does this change to `Cargo.toml` in `code` alter dependencies, features, or build settings in a way that could break builds or change behaviour for existing users, for example removing a feature, changing default features, or moving a dependency to a new major version?",
        "The manifest change can break downstream builds or change behaviour.",
        "The manifest change is additive or internal and cannot affect existing users.",
    )
    .manifest(),
    Q::noul(
        "cargo.unpinned_source",
        D::Cargo,
        "Does this change to `Cargo.toml` in `code` add a dependency from a git repository without a fixed `rev` or `tag`, from a local `path` outside the workspace, or with a `*` version?",
        "A dependency can change underneath the project without the manifest changing.",
        "Every added dependency resolves to a fixed, published version or a pinned revision.",
    )
    .gate(&[&[r"\bgit\s*=", r"\bpath\s*=", r#"=\s*"\*""#, r#"version\s*=\s*"\*""#]])
    .manifest(),
];

pub static PROFILES: &[Profile] = &[
    Profile {
        name: "tokio",
        detect_crates: &["tokio"],
        reference: "references/frameworks/tokio.md",
        verified_against: "tokio 1.x: docs.rs/tokio (task::block_in_place, task::spawn_blocking, sync::mpsc blocking_send, macro.select cancellation safety)",
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
            .gate(&[&[r"spawn\("], &[r"\b(loop|while)\b", r"interval"]])
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
                "Does the changed code in `code` use `tokio::sync::Mutex` for data that is locked only briefly and released before any `.await`, where `std::sync::Mutex` would be simpler and faster?",
                "An async mutex guards data that is always released before the next `.await`.",
                "The async mutex is held across `.await`, or a std mutex is already used.",
            )
            .gate(&[&[r"tokio::sync::(Mutex|RwLock)", r"\b(Mutex|RwLock)<"]])
            .threshold(0.6),
        ],
    },
    Profile {
        name: "axum",
        detect_crates: &["axum"],
        reference: "references/frameworks/axum.md",
        verified_against: "axum 0.8: docs.rs/axum/latest/axum/middleware/index.html#ordering and extract::Extension",
        questions: &[
            Q::noul(
                "axum.error_exposure",
                D::Security,
                "Does a changed Axum handler or error type in `code` send internal error details, such as database or I/O error messages, to the client, or return a success status code when the operation failed?",
                "Internal error details reach the HTTP response, or a failure is reported with a success status.",
                "Errors map to appropriate status codes with messages safe for clients.",
            )
            .gate(&[&[r"IntoResponse", r"StatusCode", r"Json\("]]),
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
                "Does the changed code in `code` use `Extension` to pass application state that may be missing for some routes, which fails at runtime with a 500 error, whereas a missing `State` is caught at build time?",
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
            .gate(&[&[r"async\s+fn"], &[r"std::fs|\bfs::", r"hash", r"bcrypt", r"argon2", r"diesel", r"rusqlite", r"thread::sleep", r"blocking"]]),
        ],
    },
    Profile {
        name: "dioxus",
        detect_crates: &["dioxus"],
        reference: "references/frameworks/dioxus.md",
        verified_against: "dioxus 0.7: dioxuslabs.com/learn/0.7 (signals, hooks, use_reactive, server functions, use_server_future reactivity)",
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
                "Every read guard is released before the same signal is written.",
            )
            .gate(&[&[r"\.read\(\)", r"\.with\(", r"\.iter\(\)"], &[r"\.write\(\)", r"\.set\(", r"with_mut", r"\w\s*\+=", r"\.push\("]]),
            Q::noul(
                "dioxus.effect_loop",
                D::Correctness,
                "Does a changed `use_effect` or `use_memo` in `code` write to a signal that it also reads without `peek()`, so that it can keep re-running itself?",
                "The effect or memo writes a signal it subscribes to.",
                "The effect or memo only writes signals it reads through `peek()` or does not read.",
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
                "The closure only uses signals or values that stay the same for the component's lifetime.",
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

// ---- verification questions (precision stage) --------------------------------
//
// Three answers, each read on its own:
// - `support`  decides whether a finding is reported (`P(supported)` against
//   the dimension's report bar) and, separately, whether Jev could judge it
//   at all. `insufficient_context` is not a refutation: cross-file findings
//   (lock ordering, semver breaks) land there, and the report should say
//   "Jev could not verify this from local context" rather than drop them.
// - `severity` is a Score, because severity is ordered; read the mass on
//   `SEVERITY_HIGH_FROM..` for "at least high".
// - `category` separates defects from taste. It has no "not supported"
//   option on purpose: that would be the complement of `support`, and Jev
//   does not promise that complementary questions agree.

pub const VERIFY_SUPPORT: &str = "support";
pub const VERIFY_SEVERITY: &str = "severity";
pub const VERIFY_CATEGORY: &str = "category";

pub const SUPPORTED: &str = "supported";
pub const REFUTED: &str = "refuted";
pub const INSUFFICIENT_CONTEXT: &str = "insufficient_context";

pub const SUPPORT_OPTIONS: &[(&str, &str)] = &[
    (
        SUPPORTED,
        "The lines marked `>` in `code` contain the defect that `claim` describes.",
    ),
    (
        REFUTED,
        "`code` shows that the defect is absent, or `claim` misreads what the code does.",
    ),
    (
        INSUFFICIENT_CONTEXT,
        "Whether `claim` is true depends on code that is outside `code`.",
    ),
];

/// `(name, description)`, from the low end of the scale to the high end.
/// The index is the level Jev reports.
pub const SEVERITY_LEVELS: &[(&str, &str)] = &[
    (
        "low",
        "A minor issue with little practical impact, such as a small inefficiency.",
    ),
    (
        "medium",
        "A defect that needs unusual conditions, noticeably degrades performance, or makes failures hard to diagnose.",
    ),
    (
        "high",
        "Wrong results, a crash or panic, or a resource leak in a realistic situation.",
    ),
    (
        "critical",
        "Undefined behaviour, memory unsafety, a deadlock, data loss or corruption, or a security vulnerability reachable in normal use.",
    ),
];
pub const SEVERITY_HIGH_FROM: usize = 2;

pub fn severity_name(level: usize) -> Option<&'static str> {
    SEVERITY_LEVELS.get(level).map(|(name, _)| *name)
}

pub const CATEGORY_OPTIONS: &[(&str, &str)] = &[
    (
        "real_defect",
        "The claim describes code that can misbehave, crash, leak, be unsound, or be insecure.",
    ),
    (
        "debatable_tradeoff",
        "The claim describes a reasonable design or performance trade-off that people could disagree on.",
    ),
    (
        "style_preference",
        "The claim is about naming, formatting, or code style rather than behaviour.",
    ),
];

/// A `debatable_tradeoff` claim is reported only if Jev still gives
/// `real_defect` at least this much probability.
pub const TRADEOFF_REAL_DEFECT_BAR: f64 = 0.40;

/// `style_preference` dismisses a claim only when the category answer is at
/// least this confident. A narrow style win on a well-supported claim falls
/// through to `uncertain` instead of being thrown away.
pub const STYLE_DISMISS_MIN_CONFIDENCE: f64 = 0.50;

fn choice_json(instructions: &str, options: &[(&str, &str)]) -> serde_json::Value {
    let criteria: serde_json::Map<String, serde_json::Value> = options
        .iter()
        .map(|(key, description)| ((*key).to_owned(), (*description).into()))
        .collect();
    serde_json::json!({"type": "choice", "instructions": instructions, "criteria": criteria})
}

fn score_json(instructions: &str, levels: &[&str]) -> serde_json::Value {
    serde_json::json!({"type": "score", "instructions": instructions, "criteria": levels})
}

pub fn verify_questions() -> serde_json::Value {
    let severity_levels: Vec<&str> = SEVERITY_LEVELS.iter().map(|(_, d)| *d).collect();
    serde_json::json!({
        VERIFY_SUPPORT: choice_json(
            "Does `code` contain the defect that `claim` describes? Judge only the lines marked `>` and the code they depend on.",
            SUPPORT_OPTIONS,
        ),
        VERIFY_SEVERITY: score_json(
            "Assume `claim` is true. How severe is the defect it describes?",
            &severity_levels,
        ),
        VERIFY_CATEGORY: choice_json(
            "What kind of problem does `claim` describe?",
            CATEGORY_OPTIONS,
        ),
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
        Primitive::Score { levels, .. } => score_json(q.instructions, levels),
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

    /// Ordinary code in unit form: a marker column, an attribute, `?`,
    /// `String`, `bool`, `->`, a reference, a generic. None of it is a
    /// reason to ask a gated question.
    const PLAIN: &str = "\
 use std::fmt::Write;

-#[derive(Debug)]
+#[derive(Debug, Clone)]
 struct Greeter {
+    name: String,
+    loud: bool,
 }

 impl Greeter {
+    fn greet(&self, out: &mut String) -> Result<(), std::fmt::Error> {
+        let items: Vec<&str> = vec![\"a\", \"b\"];
+        let suffix = if self.loud { \"!\" } else { \".\" };
-        write!(out, \"hi\")?;
+        write!(out, \"hi {}{suffix} {items:?}\", self.name)?;
+        Ok(())
+    }
 }
";

    fn open_ids(text: &str) -> HashSet<&'static str> {
        all_specs()
            .filter(|(q, _)| !q.gate.is_empty() && q.gate_open(text))
            .map(|(q, _)| q.id)
            .collect()
    }

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
                assert!(!group.is_empty(), "{} has an empty gate group", q.id);
                RegexSet::new(group.iter()).unwrap_or_else(|e| panic!("{}: {e}", q.id));
            }
        }
        // Forces the lazy table, so a bad pattern fails here and not at runtime.
        assert_eq!(GATES.len(), all_specs().count());
    }

    #[test]
    fn gates_stay_closed_on_plain_code() {
        // A struct definition is a fair reason to ask about type design.
        let expected: HashSet<&str> = HashSet::from(["type_design.loose_types"]);
        assert_eq!(open_ids(PLAIN), expected);
    }

    #[test]
    fn diff_markers_alone_open_nothing() {
        // Triage units use `+`, `-` and ` ` in the first column; verification
        // excerpts add a claim column in front (`>+`, ` -`, `> `).
        let markers = "+\n-\n+    \n-    \n>\n>+\n -\n> \n+ let total: usize = count;\n";
        assert_eq!(open_ids(markers), HashSet::new());
        // A marker at the start of a line is not an operator, even after a
        // line that ends in an operand.
        let after_operand = " let n: usize = f(x)\n+    let y = z;\n let w = v\n-    let u = t;\n";
        assert_eq!(open_ids(after_operand), HashSet::new());
    }

    #[test]
    fn gates_open_on_the_code_they_target() {
        let cases: &[(&str, &str)] = &[
            ("correctness.bounds", "+    let first = items[0];"),
            ("correctness.bounds", "+    let last = v[v.len() - 1];"),
            (
                "correctness.overflow",
                "+    let left: usize = total - used;",
            ),
            ("correctness.overflow", "+    count += step as u32;"),
            (
                "correctness.wildcard",
                "+    match kind {\n+        _ => Mode::Fast,",
            ),
            (
                "error_handling.panic",
                "+    let port = args[1].parse::<u16>().unwrap();",
            ),
            ("error_handling.lossy", "+    .map_err(|_| AppError::Io)?;"),
            (
                "async.guard_across_await",
                "+    let g = state.lock().unwrap();\n+    fetch().await;",
            ),
            (
                "unsafe.transmute",
                "+    unsafe { std::mem::transmute::<u32, f32>(bits) }",
            ),
            ("ffi.unwind", "+pub extern \"C\" fn run(p: *const u8) {"),
            (
                "performance.repeated_work",
                "+    for l in lines {\n+        let re = Regex::new(p)?;",
            ),
            (
                "security.tls_verification",
                "+    .danger_accept_invalid_certs(true)",
            ),
            (
                "security.weak_randomness",
                "+    let token = rand::random::<u64>();",
            ),
            (
                "testing.weak_assertion",
                "+#[tokio::test]\n+async fn works() {",
            ),
            (
                "cargo.unpinned_source",
                "+foo = { git = \"https://example.com/foo\" }",
            ),
        ];
        for (id, text) in cases {
            assert!(open_ids(text).contains(id), "{id} should open on {text:?}");
        }
    }

    #[test]
    fn every_dimension_can_be_flagged() {
        let covered: HashSet<Dimension> = all_specs().map(|(q, _)| q.dimension).collect();
        for d in Dimension::ALL {
            assert!(covered.contains(d), "{} has no question", d.name());
        }
    }

    #[test]
    fn wording_follows_jev_guidance() {
        const BANNED: &[&str] = &["unless", "how many", "line number", "compile", "not un"];
        for (q, _) in all_specs() {
            let i = q.instructions.to_lowercase();
            if q.unit == UnitKind::RustCode {
                assert!(i.contains("`code`"), "{} must reference `code`", q.id);
            }
            assert!(q.instructions.len() < 420, "{} is too long", q.id);

            // Criteria are read as literally as instructions are.
            let mut texts = vec![q.instructions];
            match q.primitive {
                Primitive::Noul { yes, no } => {
                    texts.extend([yes, no]);
                    assert!(
                        !yes.to_lowercase().starts_with("no "),
                        "{}: the `true` criterion must state the defect positively",
                        q.id
                    );
                }
                Primitive::Score { levels, bad_from } => {
                    texts.extend(levels);
                    assert!(levels.len() >= 2, "{} needs at least two levels", q.id);
                    assert!(
                        (1..levels.len()).contains(&bad_from),
                        "{}: bad_from must leave a good level and a bad level",
                        q.id
                    );
                }
            }
            for text in texts {
                let lower = text.to_lowercase();
                for banned in BANNED {
                    assert!(!lower.contains(banned), "{} contains {banned:?}", q.id);
                }
            }
        }
    }

    #[test]
    fn names_roundtrip_through_parse_and_serde() {
        for d in Dimension::ALL {
            assert_eq!(Dimension::parse(d.name()), Some(*d));
            assert_eq!(
                serde_json::to_value(d).unwrap(),
                serde_json::json!(d.name())
            );
        }
        assert_eq!(
            Dimension::parse(" Error-Handling "),
            Some(Dimension::ErrorHandling)
        );
        assert_eq!(Dimension::parse("errors"), Some(Dimension::ErrorHandling));
        assert_eq!(Dimension::parse("nope"), None);
    }

    #[test]
    fn thresholds_are_probabilities() {
        let inside = |t: f64| t > 0.0 && t < 1.0;
        for d in Dimension::ALL {
            assert!(inside(d.default_triage_threshold()), "{}", d.name());
            assert!(inside(d.default_report_threshold()), "{}", d.name());
        }
        for (q, _) in all_specs() {
            assert!(inside(q.triage_threshold()), "{}", q.id);
        }
    }

    #[test]
    fn profiles_cite_their_sources() {
        for p in PROFILES {
            assert!(!p.verified_against.is_empty(), "{}", p.name);
            assert!(!p.detect_crates.is_empty(), "{}", p.name);
            assert!(p.reference.ends_with(".md"), "{}", p.name);
        }
    }

    #[test]
    fn verify_questions_have_the_api_shape() {
        let v = verify_questions();
        assert_eq!(v[VERIFY_SUPPORT]["type"], "choice");
        assert_eq!(v[VERIFY_SEVERITY]["type"], "score");
        assert_eq!(v[VERIFY_CATEGORY]["type"], "choice");

        let support: Vec<&String> = v[VERIFY_SUPPORT]["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(support, [SUPPORTED, REFUTED, INSUFFICIENT_CONTEXT]);

        let levels = v[VERIFY_SEVERITY]["criteria"].as_array().unwrap();
        assert_eq!(levels.len(), SEVERITY_LEVELS.len());
        assert_eq!(severity_name(0), Some("low"));
        assert_eq!(severity_name(SEVERITY_HIGH_FROM), Some("high"));
        assert_eq!(severity_name(levels.len()), None);

        // No option may restate the support question.
        let category = v[VERIFY_CATEGORY]["criteria"].as_object().unwrap();
        assert!(!category.contains_key("not_supported"));
    }
}
