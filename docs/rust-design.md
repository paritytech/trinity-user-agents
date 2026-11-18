# Rust design rules

These rules cover design only. Formatting belongs to `cargo +nightly fmt`
and lint-shaped rules belong to clippy, so neither is repeated here. The repo
conventions in `AGENTS.md` (module layout, `core::` over `std::`,
`truapi::latest` imports, no parallel `Native*` types) still apply on top.

The goal is to keep design debt out of the codebase. Readability comes first:
this is not systems code, and no rule here justifies making code harder to
read. When a rule would make the code worse, say why in the PR rather than
silently breaking it.

## Library or application

Library crates follow the API rules as binding: `truapi`, `truapi-provider`,
`truapi-verifiable`, `trinity-stack`, `truapi-macros`.

Application crates treat them as strong recommendations and favour the
simplest thing that works: `truapi-host-cli`, `truapi-codegen`,
`uniffi-bindgen-cli`, and binaries under `truapi-provider/src/bin`.

## Modules and imports

* Name a module after its primary item: `Foo` lives in `foo.rs`.
* Keep non-leaf modules to declarations, re-exports and small glue. Put
  implementations in leaf modules.
* No glob imports, except `use super::*` in an inline test module, `use
  Enum::*` inside code that is mostly about that enum, and `pub use child::*`
  in a non-leaf module. Do not define preludes.
* Import types and use them unqualified (`Url`, not `url::Url`) unless the name
  is ambiguous (`io::Error`). Call free functions through their module
  (`mem::take(x)`, not an imported `take(x)`).
* Import a trait inside the function that needs it unless the file uses it
  throughout.

## Ownership

* A `clone` is fine when it keeps the code simple. Borrow when it is just as
  easy to read, but do not add lifetimes, restructure ownership or contort
  control flow to save one.
* Every new `Arc` or `Rc` needs a reason the owner cannot be unique. Shared
  async runtime state is a valid reason; convenience is not.

## Function signatures

* Accept `&str`, `&[T]`, `&T` and `Option<&T>`, never `&String`, `&Vec<T>`,
  `&Box<T>` or `&Option<T>`.
* Use an enum instead of a `bool` parameter whose meaning is not obvious at the
  call site. If a `bool` stays, the call site carries `/* name */` comments.
* Library code may return `impl Iterator` and accept `impl IntoIterator` when
  callers only iterate. Application code returns and accepts `Vec` and slices.
* `impl Into<T>` parameters are for library call-site ergonomics only.

## Errors and panics

* `Option` for an obvious single failure, `Result` with a typed error when the
  caller needs to know why, `panic` only for a broken invariant (a bug).
* No stringly typed errors in library crates. Error enums carry the data a
  caller needs to react; implement `Display` and mark them `#[must_use]`.
  Make them `Copy` when they can be.
* Prefer `expect("why this cannot fail")` over `unwrap()` outside tests.
* Use `if let`, `let else` or `match` instead of checking and then unwrapping.
* `catch_unwind` only at a host entry point as defence in depth, and never as
  the thing a property relies on. Release builds use `panic = "abort"`.
* Never reach into a poisoned mutex.
* Document the panic cases of a public function under `# Panics`.

## Types

* Use a newtype when two values could be confused, carry different meaning,
  or deserve their own API. A type alias only renames; do not alias just to
  rename (see `AGENTS.md`).
* Prefer named fields over tuples and tuple structs, except single-field
  newtypes and very local code.
* Options structs with all public fields implement `Default`.

## Traits

* Define a trait only for generic code, for `impl Trait` or `dyn Trait`, or as
  an `Ext` extension trait. Do not introduce a trait to organize methods or to
  make one implementation mockable; use an inherent `impl`.
* Implement the standard trait instead of a lookalike method: `FromStr`,
  `Default`, `From`, `Display`, `TryFrom`. Pair an important `From` with a
  named constructor.
* `derive` whenever it gives the right behaviour. A hand-written impl that
  matches the derive is noise.
* Implement `Clone` for most types, `Default` when there is a natural empty
  value or a `new()` with no arguments, and `Hash` plus `Eq` on map keys.
  Implement `Copy` only if the type will stay `Copy`.
* Prefer generics over `dyn Trait` in library parameters, except in code that
  ships to WASM where monomorphization cost matters, and where the trait is
  not `dyn` compatible anyway.

## Macros

Reach for, in order: plain functions, types and traits, then `macro_rules!`,
then a build-time generator, then a proc macro. Each step down costs IDE
support, searchability and readability, so the PR says why the earlier step
was not enough.

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
* `match` lists every variant it handles explicitly; use `_` only for the
  variants it deliberately ignores.

## Unsafe

Avoid it. When it is unavoidable, isolate it behind a safe API away from
business logic, put a `// SAFETY:` comment on every block explaining why it is
sound, and justify why safe code does not work. A performance justification
comes with a benchmark.

## Debt

* A `TODO` links to an issue.
* No compatibility shims, duplicated types or parallel code paths unless the
  task asked for them.
* A new dependency, a new crate, or a new public trait in a library crate is a
  design decision. Call it out in the PR description.
