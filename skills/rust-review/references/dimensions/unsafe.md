# Unsafe code and soundness

Unsafe findings are the **highest-precision category**. A false "this is unsound" wastes an expert's time and erodes trust. A missed one is undefined behaviour. **Claim unsoundness only with a concrete argument that names the input or interleaving that breaks an invariant.**

Every changed `unsafe` block gets focused review.

## Look for

- **Safety documentation.** Every `unsafe` block needs a `// SAFETY:` comment, and every `unsafe fn` needs a `# Safety` doc section. They should state the invariants relied on, and the code must actually uphold them.
- **Safe functions with unchecked preconditions.** A safe `pub fn` that passes caller input to `get_unchecked`, `from_raw_parts`, `from_utf8_unchecked`, or pointer arithmetic without checking it is unsound. Any safe caller can trigger UB.
- **Pointer validity:**
  - null;
  - dangling (use after free, pointer to a moved or dropped local);
  - out of bounds;
  - misaligned (`read` versus `read_unaligned`).
- **Aliasing.** Two `&mut` to the same data, or a `&mut` created while a `&` is live, including via `as *mut` round trips.
- **Initialisation.**
  - `mem::uninitialized` or `zeroed` for types where zero is invalid (references, `NonNull`, enums, `bool`).
  - Reading a `MaybeUninit` before it is written.
  - `set_len` before initialising.
- **`transmute`** between types with different layout or validity invariants, or lifetime extension via transmute.
- **`repr` assumptions.** Layout depends on `#[repr(C)]` or `#[repr(transparent)]` being present.
- **Provenance.** Integer-to-pointer casts. Pointers derived from one allocation and used to access another.
- **Unwind safety.** A panic between establishing and restoring an invariant, for example inside a manual `Vec` manipulation after `set_len`.

## Do not flag

- `unsafe` with a correct `SAFETY` comment whose invariant the code checks just above. For example, a bounds check followed by `get_unchecked` in the same function.
- A missing SAFETY comment on otherwise obviously sound code. Report it at most as low severity, and never as unsoundness.

## Evidence that makes it a finding

A call from safe code that causes UB. For example: "`Table::lookup(t.values.len())` reads one element past the allocation." If you cannot write that call, the finding is at most "SAFETY comment missing".
