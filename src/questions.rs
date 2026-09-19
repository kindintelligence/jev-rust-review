//! Every Jev question and every default threshold lives in this file, so the
//! part of the system that most needs human review can be read in one place.
//!
//! Design rules, from the Jev 1.13 jaggedness page:
//! - literal, single-hop instructions that name the state field (`code`);
//! - one defect per question: fan-out is cheap, and a specific id gives the
//!   reviewer a specific place to look;
//! - Noul criteria mirror the instruction (`true` = the defect is present);
//! - no counting, arithmetic or line numbers;
//! - nothing a tool can answer. rustc, Clippy (with the extra lints in
//!   `cargo_tools::EXTRA_LINTS`) and cargo-semver-checks report their own
//!   defects as facts. Every question says in `beyond_tooling` why none of
//!   them answers it, and lists in `tool_overlap` the lints that come close.
//!   A flag or finding on lines where one of those lints fired is dropped,
//!   so no defect is reported twice;
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
    /// One sentence: why no compiler check, lint or cargo tool answers this
    /// question. Required; `every_question_goes_beyond_tooling` fails when
    /// it is empty.
    pub beyond_tooling: &'static str,
    /// Lints and tools that report part of this defect, by the name they
    /// print (`clippy::map_err_ignore`, `unused_must_use`,
    /// `cargo-semver-checks`). When one of them fired on the same lines, the
    /// tool's report stands and this question's flag is dropped.
    pub tool_overlap: &'static [&'static str],
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
            beyond_tooling: "",
            tool_overlap: &[],
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
            beyond_tooling: "",
            tool_overlap: &[],
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
    const fn beyond(mut self, why: &'static str) -> Self {
        self.beyond_tooling = why;
        self
    }
    const fn overlaps(mut self, lints: &'static [&'static str]) -> Self {
        self.tool_overlap = lints;
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

/// The name `tool_overlap` uses for cargo-semver-checks, which is a tool and
/// not a lint.
pub const SEMVER_CHECKS: &str = "cargo-semver-checks";
/// Dioxus's own checker, which this server does not run.
pub const DX_CHECK: &str = "dx check";

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
    .skip(NON_PROD)
    .beyond("Clippy's correctness lints match fixed shapes such as `x == x`; whether a condition does what the function is for depends on intent, which no tool has.")
    .overlaps(&["clippy::eq_op", "clippy::if_same_then_else", "clippy::absurd_extreme_comparisons", "clippy::overly_complex_bool_expr"]),
    Q::noul(
        "correctness.bounds",
        D::Correctness,
        "Can an index, a slice range, or a length calculation in the changed lines of `code` go out of bounds or be off by one?",
        "Some reachable input makes an index or range exceed the valid bounds, or miss the first or last element.",
        "Every index and range stays within bounds and covers exactly the intended elements.",
    )
    .gate(&[&[INDEXING, r"split_at|get_unchecked|\.windows\(|\.chunks", r"len\(\)\s*[-+]\s*\d"]])
    .skip(NON_PROD)
    .beyond("rustc proves only constant indexes out of range, and `indexing_slicing` flags every index alike; which inputs reach an index is a question about the callers' data.")
    .overlaps(&["unconditional_panic", "clippy::out_of_bounds_indexing", "clippy::indexing_slicing"]),
    // Clippy's cast lints find every lossy `as`. What is left is whether the
    // value can be out of range in practice.
    Q::noul(
        "correctness.cast",
        D::Correctness,
        "For an `as` cast in the changed lines of `code`, can the value being cast realistically be too large or negative for the target type, for example a length, a count, or a number read from input?",
        "A cast value can realistically be outside the target type's range, so the result is silently wrong.",
        "Every cast value always fits the target type, or the truncation is clearly intended.",
    )
    .gate(&[&[r"\bas\s+(u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64|char)\b"]])
    .skip(NON_PROD)
    .beyond("Clippy flags every `as` cast that could lose data; it cannot tell a length that is always small from one an attacker controls.")
    .overlaps(&["clippy::cast_possible_truncation", "clippy::cast_sign_loss", "clippy::cast_possible_wrap"]),
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
    .skip(NON_PROD)
    .beyond("rustc catches overflow only in constant expressions, and `arithmetic_side_effects` flags every operator; which operands can be large is a question about the data.")
    .overlaps(&["arithmetic_overflow", "clippy::arithmetic_side_effects"]),
    // `let _ = mutex.lock()` is Clippy's (`let_underscore_lock`, deny by
    // default). This asks about drop points the code relies on.
    Q::noul(
        "correctness.drop_order",
        D::Correctness,
        "Does the changed code in `code` rely on a value being dropped at one point when it is really dropped earlier or later, for example a temporary that lives to the end of its statement, a guard released by an early `drop`, or fields dropped in declaration order?",
        "A value's drop point differs from what the surrounding code relies on.",
        "Every value lives exactly as long as the code relies on.",
    )
    .gate(&[&[r"\bdrop\(", r"impl\s+Drop", r"_guard", r"mem::forget", r"\.lock\(\)"]])
    .skip(NON_PROD)
    .beyond("Clippy catches a guard bound to `_`; whether later code depends on a value still being alive is a property of the program's logic.")
    .overlaps(&["clippy::let_underscore_lock", "clippy::let_underscore_must_use", "clippy::significant_drop_in_scrutinee"]),
    // ---- ownership ---------------------------------------------------
    // Clippy finds the clone whose value is never used again
    // (`redundant_clone`) and the needless `to_owned`. A function that takes
    // ownership and only reads is `needless_pass_by_value`, so that question
    // is gone.
    Q::noul(
        "ownership.clone",
        D::Ownership,
        "Do the changed lines in `code` copy a large value, or copy a value on every pass of a loop, where the code only reads the copy and a borrow would do?",
        "An avoidable copy of a large value, or a copy repeated inside a loop.",
        "Every copy is needed, or it is cheap: an `Arc` or `Rc` clone, a small value, or data moved into a spawned task or thread.",
    )
    .gate(&[&[r"\.clone\(\)", r"\.to_owned\(\)", r"\.to_vec\(\)", r"\.to_string\(\)", r"String::from", r"\.cloned\(\)"]])
    .skip(NON_PROD)
    .beyond("Clippy proves a clone redundant only when the original is never used again; whether a copy that is used is large or hot enough to matter is a judgement about the data.")
    .overlaps(&["clippy::redundant_clone", "clippy::unnecessary_to_owned", "clippy::implicit_clone", "clippy::needless_pass_by_value"]),
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
    .skip(NON_PROD_OR_BUILD)
    .beyond("Clippy can count `bool` fields and parameters; it cannot know that a string or an integer stands for a closed set of states.")
    .overlaps(&["clippy::struct_excessive_bools", "clippy::fn_params_excessive_bools"]),
    // ---- error handling ------------------------------------------------
    // rustc reports an unused `Result` and Clippy reports `let _ =` on one.
    // What is left is a failure that is used, but turned into a success.
    Q::noul(
        "error_handling.swallowed",
        D::ErrorHandling,
        "Do the changed lines in `code` turn a failure into a success or a default so that it goes unnoticed, for example `.ok()` whose `None` is ignored, `unwrap_or_default()` on a `Result`, an `Err(_)` arm that does nothing, or an `if let Ok` with no `else`?",
        "A failure is silently ignored and the program continues as if the operation succeeded.",
        "Every error is handled, returned, logged, or deliberately ignored where ignoring it is correct.",
    )
    .gate(&[&[r"\.ok\(\)", r"unwrap_or_default", r"unwrap_or\(", r"Err\(_\)", r"if\s+let\s+Ok", r"\.is_ok\(\)"]])
    .skip(TESTS)
    .beyond("rustc and Clippy see a `Result` that is dropped; they do not see one that is consumed by `.ok()` or a default, and whether the default is a correct answer is a judgement.")
    .overlaps(&["unused_must_use", "clippy::let_underscore_must_use"]),
    // Clippy can find every `unwrap` when a project asks it to. Where the
    // value comes from is the part that needs judgement.
    Q::noul(
        "error_handling.panic",
        D::ErrorHandling,
        "Can the changed lines in `code` panic on input or state that the program does not control, for example through `unwrap`, `expect`, indexing, `panic!`, or `unreachable!`?",
        "A panic is reachable from external input, I/O results, or other state the program does not control.",
        "Every possible panic enforces an invariant that the surrounding code has already checked or documented.",
    )
    .gate(&[&[r"\.unwrap\(\)", r"\.expect\(", r"panic!", r"unreachable!", r"todo!", r"unimplemented!", INDEXING]])
    .skip(NON_PROD)
    .beyond("`unwrap_used` and its siblings flag every call site alike; whether the value comes from input the program does not control is not visible to a lint.")
    .overlaps(&["clippy::unwrap_used", "clippy::expect_used", "clippy::indexing_slicing", "clippy::panic", "clippy::unreachable", "clippy::todo", "clippy::unimplemented"]),
    // `.map_err(|_| ..)` is Clippy's (`map_err_ignore`).
    Q::noul(
        "error_handling.lossy",
        D::ErrorHandling,
        "Does the changed code in `code` replace an error with a new one built from a fixed variant, a fixed message, or only the old error's text, so that the original error's source or context is lost?",
        "The original error's information is discarded, so the caller cannot tell what actually failed.",
        "The original error is kept, wrapped, or chained, or it carries no useful information.",
    )
    .gate(&[&[r"map_err", r"\.ok_or", r"Box<dyn\s+(std::error::)?Error", r"anyhow!|bail!", r"impl\s+From<", r#"Err\(\s*(format!|String::|")"#]])
    .skip(TESTS)
    .beyond("Clippy sees only a closure that ignores its argument; an error that is read and then flattened to a string, or replaced in a `From` impl, passes every lint.")
    .overlaps(&["clippy::map_err_ignore"]),
    Q::noul(
        "error_handling.drop_panic",
        D::ErrorHandling,
        "Can a changed `Drop` implementation in `code` panic?",
        "The `drop` body contains an operation that can panic.",
        "The `drop` body cannot panic.",
    )
    .gate(&[&[r"impl[^{]*\bDrop\s+for"]])
    .beyond("No rustc or Clippy lint treats a `drop` body differently from any other function, so a panic there is not reported.")
    .overlaps(&["clippy::unwrap_used", "clippy::expect_used", "clippy::panic"]),
    // ---- async ---------------------------------------------------------
    // A std, parking_lot or RefCell guard held across `.await` is Clippy's
    // (`await_holding_lock`, `await_holding_refcell_ref`, both on by
    // default), so there is no question for it.
    Q::noul(
        "async.blocking_call",
        D::Async,
        "Does an `async` function or block in `code` call an operation that blocks the thread, such as `std::thread::sleep`, `std::fs` or `std::net` I/O, or a blocking HTTP client, directly instead of through `spawn_blocking`?",
        "Blocking work runs directly on the async executor thread.",
        "All blocking work is offloaded, or the operation is non-blocking or trivially short.",
    )
    .gate(&[&[r"\basync\b"], &[r"thread::sleep", r"std::fs|\bfs::", r"File::", r"std::net", r"blocking", r"read_to_string", r"\.join\(\)", r"Command::new", r"stdin", r"sync_channel", r"\.recv\(\)"]])
    .beyond("Blocking is not part of a function's type, so neither rustc nor Clippy knows which calls stall an executor thread.")
    .overlaps(&["clippy::disallowed_methods"]),
    Q::noul(
        "async.select_cancellation",
        D::Async,
        "In a `select!` in `code`, can a branch that loses the race be cancelled after it has consumed data or partly updated state, so that data is lost or state is left inconsistent?",
        "Cancelling a losing branch loses data or leaves state half-updated.",
        "Every branch is cancellation safe, or losing progress in it is harmless.",
    )
    .gate(&[&[r"select!"]])
    .beyond("Cancellation safety is documented in prose, not encoded in types, so no compiler check or lint can see a future that loses data when it is dropped."),
    Q::noul(
        "async.detached_task",
        D::Async,
        "Does the changed code in `code` spawn a task and drop its `JoinHandle`, so that nothing awaits the task, observes its errors or panics, or stops it on shutdown?",
        "A spawned task has no owner that awaits it, checks its result, or can stop it.",
        "Every spawned task is awaited, tracked in a set, or deliberately detached with its errors handled inside the task.",
    )
    .gate(&[&[r"spawn\("]])
    .skip(TESTS)
    .beyond("`JoinHandle` is not `#[must_use]`, so dropping one is silent; whether a task may run unowned is a design decision no lint knows.")
    .overlaps(&["clippy::let_underscore_future"]),
    Q::noul(
        "async.unbounded",
        D::Async,
        "Does the changed code in `code` allow work or buffering to grow without a limit, such as an unbounded channel that can be fed faster than it is drained, or one spawned task per input item with no cap?",
        "Memory or task count can grow without bound under load.",
        "Work and buffering are bounded, or the input size is small and fixed.",
    )
    .gate(&[&[r"unbounded", r"spawn\(", r"join_all", r"FuturesUnordered", r"buffer_unordered", r"for_each_concurrent"]])
    .skip(TESTS)
    .beyond("Growth under load depends on how fast producers and consumers run, which no static check models.")
    .overlaps(&["clippy::disallowed_methods"]),
    Q::noul(
        "async.sequential_awaits",
        D::Async,
        "Does the changed code in `code` await independent operations one after another inside a loop where they could safely run concurrently?",
        "Independent awaits run strictly in sequence and could run concurrently.",
        "The awaits depend on each other, must be ordered, or are few enough that order does not matter.",
    )
    .gate(&[&[r"\.await"], &[r"\b(for|while|loop)\b"]])
    .skip(NON_PROD)
    .threshold(0.55)
    .beyond("Whether two awaits are independent depends on what the awaited operations do to shared state, which no lint analyses."),
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
    ])
    .beyond("Each step is memory safe and type correct, so the compiler accepts it; the race lives in the gap between two statements, which no lint models.")
    .overlaps(&["clippy::map_entry"]),
    Q::noul(
        "concurrency.atomics",
        D::Concurrency,
        "Does the changed code in `code` use atomics incorrectly, such as `Ordering::Relaxed` where the atomic value publishes other data, or a separate `load` followed by `store` where one atomic read-modify-write is needed?",
        "An atomic ordering is too weak for how the value is used, or a compound update is not atomic.",
        "Orderings match how the values are used and every compound update is a single atomic operation.",
    )
    .gate(&[&[r"Atomic[A-Z]\w+", r"Ordering::(Relaxed|SeqCst|Acquire|Release|AcqRel)"]])
    .beyond("Every ordering is a valid argument, so the code builds clean; which ordering a use needs depends on what the value publishes."),
    // Clippy checks the fields of an `unsafe impl Send`
    // (`non_send_fields_in_send_ty`). It has no check for `Sync`, and it
    // cannot tell whether a raw pointer's target is synchronised.
    Q::noul(
        "concurrency.unsafe_send_sync",
        D::Concurrency,
        "Does a changed `unsafe impl Sync`, or a changed `unsafe impl Send` for a type that holds a raw pointer, in `code` let two threads reach the same data with no synchronisation?",
        "Two threads can reach the type's data at once and nothing synchronises them.",
        "Every access to the shared data is synchronised, or the data is never mutated after construction.",
    )
    .gate(&[&[r"unsafe\s+impl[^{]*\b(Send|Sync)\b"]])
    .beyond("Clippy checks field types of `unsafe impl Send` only; it has no `Sync` check, and whether access behind a raw pointer is synchronised is not in any type.")
    .overlaps(&["clippy::non_send_fields_in_send_ty"]),
    Q::noul(
        "concurrency.lock_scope",
        D::Concurrency,
        "Does the changed code in `code` keep a lock held while doing slow or re-entrant work that does not need the lock, such as I/O, acquiring another lock, or calling a callback?",
        "A lock is held across slow work or another lock acquisition that does not need it.",
        "Critical sections are short and only cover the data they protect.",
    )
    .gate(&[&[LOCK_CALL], &[r"Mutex", r"RwLock"]])
    .beyond("Clippy's `significant_drop_tightening` sees where a guard could be dropped sooner; it does not know which calls are slow or take another lock.")
    .overlaps(&["clippy::significant_drop_tightening", "clippy::await_holding_lock"]),
    // ---- unsafe ------------------------------------------------------------
    // Three narrow questions instead of one that lists five kinds of UB.
    // Undocumented `unsafe` is Clippy's (`undocumented_unsafe_blocks`,
    // `missing_safety_doc`). Miri finds UB only on the executions a test
    // reaches, and the report suggests it whenever `unsafe` changed.
    Q::noul(
        "unsafe.memory_access",
        D::Unsafe,
        "Can some input or call sequence make the changed `unsafe` code in `code` read or write memory it must not touch: out of bounds, through a dangling or null pointer, or before the memory is initialised?",
        "A concrete input or call sequence makes an `unsafe` operation access invalid or uninitialised memory.",
        "Every `unsafe` memory access is valid for all inputs, for example because the code checks bounds or lengths first.",
    )
    .gate(&[&[r"\bunsafe\b"]])
    .beyond("`unsafe` is exactly where the compiler stops checking; Miri finds a bad access only on an execution a test reaches, and no static tool finds the input."),
    Q::noul(
        "unsafe.aliasing",
        D::Unsafe,
        "Can the changed `unsafe` code in `code` create two live mutable references to the same data, or a mutable reference while a shared reference to the same data is still in use?",
        "Two references that Rust forbids from coexisting can be alive at the same time.",
        "References created in the `unsafe` code never alias in a forbidden way.",
    )
    .gate(&[&[r"\bunsafe\b"], &[r"&mut\b", r"\*mut\b", r"as_mut", r"from_raw", r"UnsafeCell", r"get_unchecked_mut"]])
    .beyond("The borrow checker does not follow raw pointers; rustc's `invalid_reference_casting` catches only a direct `&T` to `&mut T` cast.")
    .overlaps(&["invalid_reference_casting", "clippy::mut_from_ref"]),
    Q::noul(
        "unsafe.transmute",
        D::Unsafe,
        "Does a changed `transmute` or pointer cast in `code` convert between types whose size, alignment, layout, or valid values differ?",
        "The source and target types are not layout compatible, or the source can hold a value that is invalid for the target.",
        "The two types have the same size, alignment, and layout, and every source value is valid for the target.",
    )
    .gate(&[&[r"transmute", r"\bas\s+\*(const|mut)\b", r"\.cast::<", r"from_raw_parts"]])
    .beyond("rustc rejects a `transmute` between sizes that differ and Clippy knows a few fixed type pairs; validity of the values and layout of user types are not checked.")
    .overlaps(&["clippy::transmute_undefined_repr", "clippy::cast_ptr_alignment", "clippy::wrong_transmute", "clippy::unsound_collection_transmute"]),
    // ---- ffi ----------------------------------------------------------------
    Q::noul(
        "ffi.ownership",
        D::Ffi,
        "Does the changed FFI code in `code` free memory with a different allocator from the one that allocated it, free it twice, or leave it unclear which side of the `extern` boundary must free it?",
        "Memory crossing the boundary is freed by the wrong side, freed twice, or never freed.",
        "Each allocation crossing the boundary is freed exactly once by the side that allocated it.",
    )
    .gate(&[&[r"into_raw|from_raw", r"\bfree\(", r"Box::leak", r"mem::forget", r"CString"], &[r#"extern\s+"C""#, r"no_mangle", r"c_char", r"c_void", r"\*(mut|const)\s"]])
    .beyond("Ownership across an `extern` boundary is a convention between two languages, so no Rust tool can see which side frees a pointer."),
    Q::noul(
        "ffi.pointers_and_strings",
        D::Ffi,
        "Does the changed FFI code in `code` dereference a pointer received from foreign code before checking it for null, or pass or read a string that lacks a NUL terminator or may contain invalid UTF-8?",
        "A foreign pointer is used before a null check, or a string crosses the boundary with the wrong termination or encoding.",
        "Foreign pointers are checked for null before use and strings are converted with `CStr` or `CString` correctly.",
    )
    .gate(&[&[r"c_char", r"CStr", r"CString", r"\*(mut|const)\s", r"c_void"], &[r#"extern\s+"C""#, r"no_mangle", r"\bunsafe\b"]])
    .beyond("Clippy checks only that a function dereferencing a raw pointer argument is marked `unsafe`; a missing null check or NUL terminator passes every lint.")
    .overlaps(&["clippy::not_unsafe_ptr_arg_deref"]),
    Q::noul(
        "ffi.unwind",
        D::Ffi,
        "Can a changed `extern \"C\"` function in `code` panic, for example from `unwrap`, indexing, or an allocation, with nothing such as `catch_unwind` to stop it, so that the panic aborts the whole process?",
        "A panic can start inside the `extern \"C\"` function and nothing catches it.",
        "The function body cannot panic, or panics are caught before the boundary.",
    )
    .gate(&[&[r#"extern\s+"C"\s+fn"#]])
    .beyond("Since Rust 1.81 a panic in an `extern \"C\"` function aborts the process instead of unwinding, and no lint reports a panic path inside one.")
    .overlaps(&["clippy::unwrap_used", "clippy::indexing_slicing"]),
    // ---- performance -----------------------------------------------------------
    // A regex built inside a loop is Clippy's (`regex_creation_in_loops`).
    Q::noul(
        "performance.repeated_work",
        D::Performance,
        "Does the changed code in `code` repeat avoidable work inside a loop, such as allocating, cloning, parsing, or searching a list linearly, in a way that grows costly as the input grows?",
        "Work inside a loop could be hoisted or replaced with a lookup, and the cost grows with input size.",
        "The loop does only necessary work, or the input is small and bounded.",
    )
    .gate(&[
        &[LOOP],
        &[r"\.clone\(\)|\.to_(string|owned|vec)\(\)", r"String::(from|new)|Vec::new|format!", r"\.parse\b|from_str", r"\.contains\(|\.find\(|\.position\(", r"\.collect"],
    ])
    .skip(NON_PROD_OR_BUILD)
    .beyond("Clippy's perf lints match single expressions; a linear search inside a loop is quadratic only because of the loop around it, and how large the input gets is not in the code.")
    .overlaps(&["clippy::regex_creation_in_loops", "clippy::needless_collect", "clippy::redundant_clone"]),
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
    .skip(TESTS)
    .beyond("Clippy's style lints rewrite known patterns and its complexity lints count branches; neither measures whether a reader can follow the intent.")
    .overlaps(&["clippy::cognitive_complexity", "clippy::too_many_lines", "clippy::excessive_nesting"]),
    // ---- api -------------------------------------------------------------------
    // cargo-semver-checks answers this as a fact. The diagnostics tool runs
    // it when it is installed, and then this question's flags are dropped.
    Q::noul(
        "api.breaking_change",
        D::Api,
        "Does the change in `code` remove or alter a public (`pub`) item in a way that breaks existing callers, such as removing or renaming it, changing a function signature, adding trait bounds, adding a required trait method, or changing public fields?",
        "Code written against the old public item would stop building or behave differently.",
        "Every public item that existed before still exists with a compatible signature; only new items are added or private code changed.",
    )
    .gate(&[&[r"\bpub\s+(fn|struct|enum|trait|type|const|static|mod|use|unsafe|async)\b", r"\bpub\s+\w+\s*:"]])
    .skip(NON_LIB)
    .beyond("cargo-semver-checks does answer this, and its answer replaces this question whenever it ran; the question is the fallback for machines where it is not installed.")
    .overlaps(&[SEMVER_CHECKS]),
    // ---- macros ----------------------------------------------------------------
    Q::noul(
        "macros.double_evaluation",
        D::Macros,
        "Does a changed macro in `code` expand one of its argument expressions more than once, so that an argument with side effects runs repeatedly?",
        "A macro argument appears more than once in the expansion.",
        "Each argument is bound to a local once and the local is reused.",
    )
    .gate(&[&[r"macro_rules!"]])
    .beyond("rustc checks a macro's expansion, not its definition, and no Clippy lint looks for a fragment that is expanded twice."),
    Q::noul(
        "macros.name_collision",
        D::Macros,
        "Does a changed macro in `code` generate items, or in a procedural macro variables, with fixed names that can collide with names at the call site?",
        "Generated names can clash with names the caller already uses.",
        "Generated names are hygienic, derived from the input, or unique.",
    )
    .gate(&[&[r"macro_rules!", r"proc_macro", r"quote!"]])
    .beyond("A collision appears only in the crate that calls the macro with a clashing name, so nothing fails where the macro is defined."),
    // ---- serde -------------------------------------------------------------------
    Q::noul(
        "serde.compatibility",
        D::Serde,
        "Does the change in `code` alter how a type is serialized, such as renaming a field, changing a field's type, or adding `#[serde(untagged)]`, so that previously stored or transmitted data no longer round-trips?",
        "Data written by the old version cannot be read by the new one, or the reverse.",
        "The serialized format stays compatible with data written before the change.",
    )
    .gate(&[&[r"Serialize", r"Deserialize", r"serde\("]])
    .beyond("The wire format is not part of the Rust API, so neither rustc nor cargo-semver-checks sees a rename that breaks stored data."),
    Q::noul(
        "serde.silent_default",
        D::Serde,
        "Does a changed `#[serde(default)]`, `Option` field, or `#[serde(other)]` in `code` make deserialization accept input that is missing a required value or carries an unknown one, where that input should be rejected?",
        "Invalid or incomplete input is now accepted and filled with a default.",
        "Defaults apply only where a missing value is valid.",
    )
    .gate(&[&[r"serde\([^)]*(default|other|skip)"]])
    .beyond("Every serde attribute is valid wherever it is allowed; whether a missing value is acceptable is a rule of the data, which no tool knows."),
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
    .skip(TESTS)
    .beyond("Rust has no taint tracking: a `String` from a request and one from a constant have the same type, so no lint tells them apart."),
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
    .skip(TESTS)
    .beyond("Which values are secret is knowledge about the application; to the compiler a token is one more `String` passed to a format macro."),
    Q::noul(
        "security.tls_verification",
        D::Security,
        "Does the changed code in `code` disable TLS certificate verification or hostname verification?",
        "Certificate or hostname verification is turned off.",
        "TLS verification stays on.",
    )
    .gate(&[&[r"danger", r"accept_invalid", r"(?i)verify_?none", r"(?i)no_?verif", r"set_verify"]])
    .skip(TESTS)
    .beyond("Turning verification off is a supported API call; only a project that lists it under `disallowed_methods` hears about it.")
    .overlaps(&["clippy::disallowed_methods"]),
    Q::noul(
        "security.weak_randomness",
        D::Security,
        "Does the changed code in `code` create a secret, token, key, nonce, or password with a random number generator that is not cryptographically secure?",
        "A security-sensitive value comes from a non-cryptographic generator.",
        "Security-sensitive values come from a cryptographic source, or the random values are not security sensitive.",
    )
    .gate(&[&[r"thread_rng|\brand::|random\(|SmallRng|fastrand|StdRng::seed"]])
    .skip(TESTS)
    .beyond("A fast generator is correct for a simulation and wrong for a token; what the number is used for is not visible to a lint."),
    Q::noul(
        "security.unbounded_input",
        D::Security,
        "Does the changed code in `code` read or deserialize untrusted input without a size limit, so that a large or crafted input can exhaust memory?",
        "Untrusted input is read or parsed with no bound on its size.",
        "Input size is bounded, or the input is trusted.",
    )
    .gate(&[&[r"from_slice", r"from_reader", r"read_to_end", r"read_to_string", r"bincode", r"\.bytes\(\)\.await", r"to_bytes\("]])
    .skip(TESTS)
    .beyond("Whether a reader is trusted, and how large its input may get, is not in any type, so no tool flags an unbounded read."),
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
    .gate(&[&[r"#\[(\w+::)*test\b", r"#\[rstest", r"proptest!"]])
    .beyond("Clippy catches an assertion on a constant; a test that asserts nothing about the value it computed passes every lint and every run.")
    .overlaps(&["clippy::assertions_on_constants"]),
    // ---- cargo (manifest units) ------------------------------------------------------
    // Unpinned git, path and `*` sources are computed in code
    // (`cargo_facts`), so there is no question for them.
    Q::noul(
        "cargo.manifest_risk",
        D::Cargo,
        "Does this change to `Cargo.toml` in `code` alter dependencies, features, or build settings in a way that could break builds or change behaviour for existing users, for example removing a feature, changing default features, or moving a dependency to a new major version?",
        "The manifest change can break downstream builds or change behaviour.",
        "The manifest change is additive or internal and cannot affect existing users.",
    )
    .manifest()
    .beyond("`cargo_facts` lists what changed in the manifest and cargo resolves it; whether users downstream depend on what was removed is outside the repository."),
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
            .gate(&[&[r"block_on", r"block_in_place", r"Runtime::new", r"Builder::new_"]])
            .beyond("Whether a function already runs inside a runtime is decided by its callers at run time, so the nested `block_on` builds clean and panics later."),
            Q::noul(
                "tokio.spawn_blocking_misuse",
                D::Async,
                "Does the changed code in `code` use `spawn_blocking` for work that runs forever, or rely on aborting a `spawn_blocking` task, even though blocking tasks cannot be aborted once started?",
                "`spawn_blocking` is used for endless work or is expected to be cancellable.",
                "`spawn_blocking` is used for finite blocking work only.",
            )
            .gate(&[&[r"spawn_blocking"]])
            .beyond("`spawn_blocking` accepts any closure; that a started blocking task cannot be aborted is stated in Tokio's documentation, not in its types."),
            Q::noul(
                "tokio.no_shutdown",
                D::Async,
                "Does the changed code in `code` start a long-running task or loop that has no way to be told to stop, such as a `CancellationToken`, a shutdown channel, or a `JoinSet` that is dropped on shutdown?",
                "A long-running task or loop has no stop signal.",
                "Every long-running task can be stopped, or the task ends on its own.",
            )
            .gate(&[&[r"spawn\("], &[r"\b(loop|while)\b", r"interval"]])
            .skip(TESTS)
            .beyond("A loop with no exit is valid Rust, and Clippy's `infinite_loop` does not look for a missing shutdown signal in a spawned task.")
            .overlaps(&["clippy::infinite_loop"]),
            Q::noul(
                "tokio.select_not_cancel_safe",
                D::Async,
                "Does a `tokio::select!` branch in `code` await `read_exact`, `read_to_end`, `read_to_string`, `write_all`, `Mutex::lock`, `Semaphore::acquire`, or another future that is not cancellation safe, inside a loop where losing the race drops progress?",
                "A non-cancellation-safe future is raced in a loop, so progress can be lost.",
                "Every raced future is cancellation safe, or it is pinned outside the loop and reused.",
            )
            .gate(&[&[r"select!"]])
            .beyond("Which Tokio futures are cancellation safe is listed in the documentation of each method; nothing in their types says so."),
            Q::noul(
                "tokio.blocking_in_async",
                D::Async,
                "Does the changed code in `code` call `blocking_send`, `blocking_recv`, or `blocking_lock` from inside async code, where these methods panic?",
                "A `blocking_*` method is called from async code.",
                "`blocking_*` methods are only called from synchronous code.",
            )
            .gate(&[&[r"blocking_send", r"blocking_recv", r"blocking_lock", r"blocking_read", r"blocking_write"]])
            .beyond("The `blocking_*` methods have ordinary signatures and panic only when called on a runtime thread, so no build or lint step sees the misuse."),
            Q::noul(
                "tokio.async_mutex_unneeded",
                D::Performance,
                "Does the changed code in `code` use `tokio::sync::Mutex` for data that is locked only briefly and released before any `.await`, where `std::sync::Mutex` would be simpler and faster?",
                "An async mutex guards data that is always released before the next `.await`.",
                "The async mutex is held across `.await`, or a std mutex is already used.",
            )
            .gate(&[&[r"tokio::sync::(Mutex|RwLock)", r"\b(Mutex|RwLock)<"]])
            .threshold(0.6)
            .beyond("Both mutexes are correct here, so nothing warns; which one fits depends on whether any guard ever lives across an `.await`."),
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
            .gate(&[&[r"IntoResponse", r"StatusCode", r"Json\("]])
            .beyond("Any string is a valid response body; that it carries a database error meant for the log is not visible to a tool."),
            Q::noul(
                "axum.layer_order",
                D::Correctness,
                "Does the change in `code` add or reorder middleware so that authentication, rate limiting, timeouts, or CORS run in the wrong order, given that with `Router::layer` the layer added last runs first on the request and with `ServiceBuilder` layers run top to bottom?",
                "Middleware runs in an order that defeats its purpose.",
                "Middleware runs in an order consistent with its purpose.",
            )
            .gate(&[&[r"\.layer\(", r"route_layer", r"ServiceBuilder"]])
            .beyond("Every ordering of layers type checks; which order defeats authentication or a timeout depends on what each layer does."),
            Q::noul(
                "axum.extension_state",
                D::Correctness,
                "Does the changed code in `code` use `Extension` to pass application state that may be missing for some routes, which fails at runtime with a 500 error, whereas a missing `State` is caught at build time?",
                "`Extension` carries state that some routes may not have.",
                "State is passed with `State`, or the `Extension` is always present.",
            )
            .gate(&[&[r"Extension"]])
            .beyond("A missing `Extension` is found when a request arrives, not when the router is built, and no lint checks routes against layers."),
            Q::noul(
                "axum.blocking_handler",
                D::Async,
                "Does a changed Axum handler in `code` do blocking or CPU-heavy work, such as synchronous database calls, file I/O, or password hashing, directly in the async handler?",
                "The handler blocks the executor thread.",
                "Blocking work is offloaded or absent.",
            )
            .gate(&[&[r"async\s+fn"], &[r"std::fs|\bfs::", r"hash", r"bcrypt", r"argon2", r"diesel", r"rusqlite", r"thread::sleep", r"blocking"]])
            .beyond("Blocking is not part of a function's type, so a handler that hashes a password on the executor thread builds and lints clean.")
            .overlaps(&["clippy::disallowed_methods"]),
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
            .gate(&[&[r"\.await"], &[r"\.read\(\)", r"\.write\(\)", r"with_mut"]])
            .beyond("Clippy's `await_holding_*` lints know std, parking_lot and `RefCell` guards; a Dioxus signal guard is on its list only if the project adds it to `await-holding-invalid-types`.")
            .overlaps(&["clippy::await_holding_invalid_type", "clippy::await_holding_refcell_ref"]),
            Q::noul(
                "dioxus.read_write_overlap",
                D::Correctness,
                "In `code`, can a signal's `.read()` guard still be alive when the same signal is written, for example writing to a signal inside a loop over its own `.read()` value, which panics at runtime with an already-borrowed error?",
                "A read guard and a write to the same signal overlap.",
                "Every read guard is released before the same signal is written.",
            )
            .gate(&[&[r"\.read\(\)", r"\.with\(", r"\.iter\(\)"], &[r"\.write\(\)", r"\.set\(", r"with_mut", r"\w\s*\+=", r"\.push\("]])
            .beyond("Signals borrow at run time like `RefCell`, so the borrow checker accepts an overlap that panics when the component renders."),
            Q::noul(
                "dioxus.effect_loop",
                D::Correctness,
                "Does a changed `use_effect` or `use_memo` in `code` write to a signal that it also reads without `peek()`, so that it can keep re-running itself?",
                "The effect or memo writes a signal it subscribes to.",
                "The effect or memo only writes signals it reads through `peek()` or does not read.",
            )
            .gate(&[&[r"use_effect", r"use_memo"]])
            .beyond("Subscriptions are created at run time by reading a signal, so no static tool sees an effect that re-triggers itself."),
            Q::noul(
                "dioxus.hook_rules",
                D::Correctness,
                "Does the changed code in `code` call a hook (a function named `use_...`) conditionally, inside a loop, inside a closure or event handler, or after an early return, so the order of hook calls can change between renders?",
                "A hook call can be skipped or reordered between renders.",
                "Hooks are called unconditionally at the top level of the component, in the same order every render.",
            )
            .gate(&[&[r"\buse_[a-z_]+\("]])
            .beyond("rustc and Clippy know nothing about hook order; `dx check` does, and it is not part of a cargo build, so most changes never pass through it.")
            .overlaps(&[DX_CHECK]),
            Q::noul(
                "dioxus.stale_capture",
                D::Correctness,
                "Does a changed closure passed to `use_effect`, `use_memo`, `use_resource`, or `use_future` in `code` use a plain prop or local value (not a signal) that can change between renders, so the closure keeps using a stale value?",
                "The closure captures a changing non-signal value that is not tracked, for example without `use_reactive`.",
                "The closure only uses signals or values that stay the same for the component's lifetime.",
            )
            .gate(&[&[r"use_effect", r"use_memo", r"use_resource", r"use_future"]])
            .beyond("Capturing a plain value in a closure is ordinary Rust; that the hook will not re-run when the value changes is Dioxus behaviour no lint models."),
            Q::noul(
                "dioxus.server_fn_trust",
                D::Security,
                "Does a changed server function in `code` trust its arguments without validating them or checking that the caller is authorised, even though server functions are public HTTP endpoints?",
                "The server function acts on unvalidated input or without an authorisation check.",
                "The server function validates input and checks authorisation, or it only returns public data.",
            )
            .gate(&[&[r"#\[server", r"#\[get", r"#\[post", r"#\[put", r"#\[delete", r"#\[patch"]])
            .beyond("A server function looks like a local call, and no tool knows that its arguments arrive from an untrusted client."),
            Q::noul(
                "dioxus.untracked_dependency",
                D::Correctness,
                "In `code`, does a `use_server_future` closure read a signal only inside its `async` block, where the read is not tracked, so changes to that signal do not re-run it?",
                "A signal is read only inside the async block of `use_server_future`.",
                "Signals are read in the closure before the async block, or none are read.",
            )
            .gate(&[&[r"use_server_future"]])
            .beyond("Where a signal is read decides whether it is tracked, and that rule lives in Dioxus's runtime, not in any type or lint."),
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

/// The spec with this id, core or profile.
pub fn spec(id: &str) -> Option<&'static QuestionSpec> {
    all_specs().map(|(q, _)| q).find(|q| q.id == id)
}

/// Defects a lint states in full, so no question is asked about them at all.
/// A finding in the dimension on the lint's lines repeats the lint.
pub const TOOL_OWNED: &[(Dimension, &str)] = &[
    (D::Async, "clippy::await_holding_lock"),
    (D::Async, "clippy::await_holding_refcell_ref"),
    (D::Async, "clippy::await_holding_invalid_type"),
    (D::Ownership, "clippy::needless_pass_by_value"),
    (D::Correctness, "clippy::wildcard_enum_match_arm"),
];

/// Every lint and tool that reports a defect of this dimension: the ones its
/// questions overlap and the ones tools own outright. A finding names a
/// dimension, not a question, so verification deduplicates by this.
pub fn tool_overlap_for(d: Dimension) -> std::collections::BTreeSet<&'static str> {
    let owned = TOOL_OWNED
        .iter()
        .filter(|(od, _)| *od == d)
        .map(|(_, l)| *l);
    all_specs()
        .filter(|(q, _)| q.dimension == d)
        .flat_map(|(q, _)| q.tool_overlap.iter().copied())
        .chain(owned)
        .collect()
}

/// The dimensions a lint speaks for.
pub fn lint_dimensions(lint: &str) -> Vec<Dimension> {
    Dimension::ALL
        .iter()
        .copied()
        .filter(|d| tool_overlap_for(*d).contains(lint))
        .collect()
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
                "error_handling.panic",
                "+    let port = args[1].parse::<u16>().unwrap();",
            ),
            (
                "error_handling.lossy",
                "+    .map_err(|e| AppError::Io(e.to_string()))?;",
            ),
            (
                "error_handling.swallowed",
                "+    let cfg = load(path).unwrap_or_default();",
            ),
            (
                "async.select_cancellation",
                "+    tokio::select! {\n+        r = stream.read_exact(&mut buf) => {}",
            ),
            (
                "unsafe.transmute",
                "+    unsafe { std::mem::transmute::<u32, f32>(bits) }",
            ),
            ("ffi.unwind", "+pub extern \"C\" fn run(p: *const u8) {"),
            (
                "performance.repeated_work",
                "+    for l in lines {\n+        if seen.contains(&l) {",
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
        ];
        for (id, text) in cases {
            assert!(open_ids(text).contains(id), "{id} should open on {text:?}");
        }
    }

    /// Questions a tool now answers must not come back.
    #[test]
    fn tool_owned_defects_have_no_question() {
        for id in [
            "async.guard_across_await",
            "ownership.signature",
            "correctness.wildcard",
            "cargo.unpinned_source",
        ] {
            assert!(spec(id).is_none(), "{id} is answered by a tool");
        }
        // Patterns a lint reports must not be what opens a narrowed gate.
        let tool_owned = "+    let _ = std::fs::remove_file(p);\n+    let g = m.lock();\n";
        assert!(!open_ids(tool_owned).contains("error_handling.swallowed"));
    }

    #[test]
    fn every_question_goes_beyond_tooling() {
        for (q, _) in all_specs() {
            let why = q.beyond_tooling;
            assert!(
                why.len() >= 40 && why.ends_with('.'),
                "{}: `beyond_tooling` must say in one sentence why no tool answers it",
                q.id
            );
            assert_eq!(
                why.matches(". ").count(),
                0,
                "{}: `beyond_tooling` is one sentence",
                q.id
            );
            for lint in q.tool_overlap {
                let named = lint.strip_prefix("clippy::").unwrap_or(lint);
                assert!(
                    !named.is_empty() && !named.contains(' ') || *lint == DX_CHECK,
                    "{}: {lint:?} is not a lint or tool name",
                    q.id
                );
            }
        }
    }

    #[test]
    fn a_dimension_collects_its_questions_overlaps() {
        let lints = tool_overlap_for(Dimension::Correctness);
        assert!(lints.contains("clippy::cast_possible_truncation"));
        assert!(lints.contains("arithmetic_overflow"));
        assert!(!lints.contains("clippy::map_err_ignore"));
        assert!(tool_overlap_for(Dimension::Api).contains(SEMVER_CHECKS));
        // A guard across `.await` has no question, and is still an async
        // defect that a finding must not repeat.
        assert!(tool_overlap_for(Dimension::Async).contains("clippy::await_holding_lock"));
        assert_eq!(
            lint_dimensions("clippy::needless_pass_by_value"),
            [Dimension::Ownership]
        );
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
