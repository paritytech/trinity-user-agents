# Rust rules

How we write Rust in this repository. Formatting belongs to `make fmt` and
lints to clippy, so neither is restated here. The crate and binding
invariants in `AGENTS.md` describe this repo's architecture and apply on top.

The goal is to keep design debt out of the codebase. Readability comes first:
this is not systems code, and no rule here justifies making code harder to
read. When a rule would make the code worse, say why in the PR rather than
silently breaking it.

## Modules and visibility

* Use `foo.rs` plus an optional `foo/` directory for child modules. Do not
  add new `foo/mod.rs` files unless preserving generated output or an existing
  external convention.
* Name a module after its primary item: `Foo` lives in `foo.rs`.
* Keep non-leaf modules to declarations, re-exports and small glue. Put
  implementations in leaf modules.
* Do not use scoped visibility modifiers such as `pub(crate)`, `pub(super)` or
  `pub(in ...)`. Establish the boundary once through the module hierarchy:
  keep implementation modules private, make items `pub` inside them, and
  re-export what belongs to the public interface. Do not make a whole module
  public to expose one helper or to make a test compile.
* Prefer an inherent method when an operation belongs to an existing type
  rather than adding another free-standing export.

## Imports

* No glob imports, except `use super::*` in an inline test module, `use
  Enum::*` inside code that is mostly about that enum, and `pub use child::*`
  in a non-leaf module. Do not define preludes.
* Import types and use them unqualified (`Url`, not `url::Url`) unless the name
  is ambiguous (`io::Error`). Call free functions through their module
  (`mem::take(x)`, not an imported `take(x)`).
* Import a trait inside the function that needs it unless the file uses it
  throughout.
* In runtime code, prefer `core::` over `std::` for types `core` provides
  (`core::pin::Pin`, `core::task::Poll`, `core::fmt`). Keep `std::` for
  std-only APIs, tests and std-only programs such as `truapi-codegen`.

## Ownership

* A `clone` is fine when it keeps the code simple. Borrow when it is just as
  easy to read, but do not add lifetimes, restructure ownership or contort
  control flow to save one.
* Every new `Arc` or `Rc` needs a reason the owner cannot be unique. Shared
  async runtime state is a valid reason, convenience is not.

## Function signatures

* Accept `&str`, `&[T]`, `&T` and `Option<&T>`, never `&String`, `&Vec<T>`,
  `&Box<T>` or `&Option<T>`.
* Use an enum instead of a `bool` parameter whose meaning is not obvious at the
  call site.
* Return and accept `Vec` and slices by default. Reach for `impl Iterator`,
  `impl IntoIterator` or `impl Into<T>` only when a caller needs it.
* Prefer generics over `dyn Trait` for parameters, except in code that ships
  to WASM, where monomorphization adds size, and where the trait is not `dyn`
  compatible anyway.

## Errors and panics

* `Option` for an obvious single failure, `Result` with a typed error when the
  caller needs to know why, `panic` only for a broken invariant (a bug).
* No stringly typed errors outside binaries. Error enums carry the data a
  caller needs to react and are `#[must_use]`.
* Fix the underlying cause instead of adding a fallback that hides an error.
* Prefer `expect("why this cannot fail")` over `unwrap()` outside tests.
* Use `if let`, `let else` or `match` instead of checking and then unwrapping.
* `catch_unwind` only at a host entry point as defence in depth, and never as
  the thing a property relies on. Release builds use `panic = "abort"`.
* Never reach into a poisoned mutex.
* Document the panic cases of a public function under `# Panics`.

## Types

* Do not alias a type just to rename it, whether a primitive (`type Counter =
  u64`) or a type from another crate. Use the type directly, or a newtype when
  two values could be confused or construction has rules to enforce. An alias
  is only worth it when it captures a real abstraction.
* Reuse canonical types, constants and helpers. Do not introduce a second
  representation, or a dependency, just to convert to and from the type the
  surrounding code already uses.
* Keep one source of truth. If one field already determines a fact, do not
  carry a second one and checks to keep them in sync. Fix the representation.
* Keep fields private where they protect invariants, and do not add accessors
  that only expose implementation details.
* Prefer named fields over tuples and tuple structs, except single-field
  newtypes and very local code.

## Traits and derives

* Add abstractions and derives for concrete needs, not possible future
  callers.
* Define a trait only for generic code, for `impl Trait` or `dyn Trait`, or as
  an `Ext` extension trait. Do not introduce a trait to organize methods or to
  make one implementation mockable. Use an inherent `impl`.
* Implement the standard trait instead of a lookalike method: `FromStr`,
  `From`, `TryFrom`, `Display`. Pair an important `From` with a named
  constructor.
* Do not derive `Copy`, `Clone`, `Debug`, `PartialEq` or `Eq`, and do not
  require them as bounds, unless something uses them. Removing `Clone` can
  compile while changing behaviour, since `.clone()` on a reference clones the
  reference, so check every `.clone()` call site first.
* Do not add a synthetic `Default` to a type with no natural default.
  Construct the value at the boundary.
* When an impl is needed, derive it if the derive gives the right behaviour.
  Prefer `derive_more::Display` over a handwritten `fmt::Display` when the
  formatting is declarative.

## Macros

Reach for, in order: plain functions, types and traits, then `macro_rules!`,
then a build-time generator, then a proc macro. Each step down costs IDE
support, searchability and readability, so the PR says why the earlier step
was not enough. In codegen emitters, prefer one `indoc::writedoc!` or
`formatdoc!` with a multi-line raw string over a chain of `writeln!` calls, so
the emitted shape stays visible.

## Naming

* Constructors: `new`, `new_with_foo`, `try_new` when fallible, `from_foo`
  alongside a `From` impl.
* Conversions follow `as_` (cheap borrow), `to_` (expensive or owned), `into_`
  (consumes `self`).
* Accessors: `foo()`, `foo_mut()`, `set_foo()`, `take_foo()`,
  `replace_foo()`. No `get_` prefix.
* Ship `is_empty()` with `len()`.
* No abbreviations an outsider would not know.
* Lifetimes get descriptive names on types. Generic parameters are short and
  named after their main bound.

## API surface

* `#[must_use]` on functions whose result must not be ignored and on error
  and lazy types.
* Only mark a public function `const` if it will stay `const`.
* Do not add `#[inline]` to private or crate-internal functions. Use it only
  on small public items that need cross-crate inlining.
* Operator overloading only for mathematical or logical meaning.
* Treat `#[doc(hidden)]` items from other crates as private.
* No code before `main` (`ctor` and similar).
* Use `#[non_exhaustive]` on a struct with public fields that must be built
  through a validating constructor, and on a Rust-only enum whose consumers are
  expected to ignore variants they do not know. Do not put it on wire types or
  UniFFI types: versioned envelopes handle wire compatibility, and the Swift
  and Kotlin bindings stay exhaustive so a new variant breaks their compile.
  Public enums in `trinity-stack`, the one crate published to crates.io, may
  use it when a new variant should not be a breaking release.

## Implementation

* Annotate the type of a binding when the reader cannot see it, always for
  `.into()`, `.parse()` and `collect()`. Prefer a binding annotation over a
  turbofish.
* Keep iterator chains short. Bind a long chain to a named variable before a
  `for` loop instead of iterating inline, and avoid `for_each` without a
  reason.
* `assert!` in tests and invariants carries a message that explains why the
  condition must hold, not one that repeats it.
* `match` lists every variant it handles explicitly. Use `_` only for the
  variants it deliberately ignores.
* In format strings, inline the variables: `"value: {value:?}"`, not
  `"value: {:?}", value`.

## Lints, features and unsafe

* Treat warnings as errors. Fix the cause of a lint and never silence it with
  `#[allow]` or `#[expect]`. The one exception is a signature fixed by a
  third-party trait, such as tungstenite's callback, where the allow sits on
  that item with a comment naming the trait.
* For cast lints, change the source type first, then widen the destination.
  Prefer `try_from` over `as`.
* After a bulk regex edit, reread every signature it touched.
* Cargo features are additive. A feature compiled in with no runtime opt-in
  behaves like the feature off, because Cargo unifies features across the
  build. Build modes that change behaviour or ABI go behind `--cfg`, declared
  with check-cfg, not a cargo feature.
* Production builds carry no debug-only behaviour.
* Use a safe API when one exists. When unsafe is unavoidable, isolate it
  behind a safe API away from business logic, write one unsafe operation per
  `unsafe {}` block with its own `// SAFETY:` comment stating the invariant
  and why it holds, and do not mark a function `unsafe` if it can check its
  preconditions itself. A performance justification comes with a benchmark.

## Docs

* Every `pub` item carries a doc comment: a one-line `///` summary, a blank
  `///` line, then a short body stating the contract, units and failure
  conditions. Focus on intent and invariants, not what the signature says.
* Item-level notes go in `///` on the item. Use `//` only for line-local
  reasoning, since `//` does not show in rustdoc or IDE hover.
* Do not repeat trait docs on impls.
* Every `pub mod` has a `//!` block with its purpose and entry point.
* Put identifiers in backticks and intra-doc links in brackets. Use `Self::`
  for siblings.
* A link target that exists only under some cfg is plain code, not a link, so
  the link does not break in the other configurations.
* After rewrapping doc prose, check line starts. `+ `, `- ` and `1. ` turn a
  line into a list item, and `[x]:` at the start of a paragraph is parsed as a
  link definition and dropped.
* After API, rename or cfg changes, run
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` for each cfg and feature
  combination. Check, clippy and tests do not catch broken intra-doc links.

## Editing existing Rust

Preserve the local style. Do not add semicolons to `return`, `break` or
`continue` where the file omits them, do not add braces to match arms or
`if`/`else` written without them, and do not move operators between the end
of one line and the start of the next, except where the formatter requires
it. Format with `make fmt`, which uses the pinned nightly and includes the
runtime module tree, and keep formatting to files you changed.

## Debt

* A `TODO` links to an issue.
* Keep dependencies in the workspace and inherit them from member crates. A
  new dependency, a new crate, or a new public trait is a design decision, so
  call it out in the PR description.
