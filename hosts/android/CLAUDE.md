# PolkadotApp Agent Guide

This file is intentionally **thin**. Detailed architecture and code rules are in `.claude/docs/` and are lazy-loaded by skills, not auto-included here.

## Build prerequisites

The build needs the TrUAPI Rust core, because `:feature:products:impl` depends on `:bindings:truapi-host` in every variant. Inside `trinity-user-agents` the core is the enclosing repository and nothing has to be fetched. Outside it, run `scripts/setup-truapi.py` once: it clones `paritytech/trinity-user-agents` at the `truapi_ref` pin from `.github/actions/install/action.yaml` and writes `truapi.dir` to `local.properties`. Re-run it after the pin moves. A `truapi.dir` that is set but does not resolve still fails at configuration time rather than falling back.

`FIRESTORE_DATABASE_ID` must also be in `local.properties`, or configuration fails before anything compiles. It is a CI secret, so ask for the value.

The core generates part of its Rust sources at build time from rustdoc JSON, which needs the nightly toolchain pinned as `truapi.rustNightly` in `gradle.properties`: `rustup toolchain install "$(sed -n 's/^truapi.rustNightly=//p' gradle.properties)" --profile minimal --component rustfmt` once (the codegen formats what it emits with that toolchain's own rustfmt, which `--profile minimal` alone leaves out). Gradle runs the codegen itself.

JDK 21. `export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"` if `java` is not on PATH.

## How to work on this codebase

1. **Plan first** with `/architect`. Loads `.claude/docs/architecture/*.md` on demand. Output is a plan that names modules touched, seams used, layer placement, and north-star alignment.
2. **Implement** with `/implementer`. Loads `.claude/docs/code/*.md` on demand and writes code that follows established patterns.
3. **Review** with `/reviewer` (optionally `/reviewer <PR#>`). Loads `.claude/docs/review/*.md` plus the architecture+code docs that match the touched files; outputs a comment list tagged blocking / major / minor.

Index of all docs: `.claude/docs/README.md`.

For PR reviews from CI or against a branch:
- `/reviewer` with no arg — review the current branch's diff against `master`.

## Always-on rules

These are the cross-cutting rules that apply to **all** code writing, regardless of skill. They live here because the cost of forgetting them is high.

1. **Strings** — extract every UI string to `common/src/main/res/values/strings.xml`. Import as `import io.paritytech.polkadotapp.common.R as RCommon` and reference `RCommon.string.…`.
2. **Design-system components** — priority `Polkadot*` (migration target) > `Nova*` (legacy) > themed Material (last resort). Pick the highest-priority component that exists; raw Material only when no DS component covers the need.
3. **Spacers** — `VerticalSpacer { spacingN }` / `HorizontalSpacer { spacingN }`. Never `Spacer(Modifier.height(...))`.
4. **NovaTheme.spacings** is for paddings and margins only; not for radii, sizes, or stroke widths.
5. **Modifier** is always the first parameter; never mutate a passed-in `modifier` — apply on the caller side or build the whole modifier internally.
6. **No early return** inside a `@Composable`. Wrap in `if`/`when`.
7. **Single state per ViewModel**, derived via `combine` / `map` / `flatMapLatest`. Never `.copy(...)`-patch from multiple methods.
8. **Result<T>** for fallible domain operations. **`getOrThrow()` is forbidden everywhere** except `Worker.doWork()` (the Result → WorkManager-Result seam) and test code. Use `flatMap`, `mapCatching`, `withLoading("Tag")`. Severity: `major` in source docs; `blocking` in a ViewModel / UI mapper / main-path code.
9. **`impl` modules never depend on other `impl` modules.** Cross-feature wiring goes through `api`. Watch for logical cycles too — extract shared logic into a more general module.
10. **Interactors** live in `feature/<X>/impl/domain/<screenName>/`, one per ViewModel.
11. **`sealed interface`** over `sealed class` when no constructor args are needed.
12. **Package leaves** are camelCase (`pairRequest`, not `pairrequest`).
13. **No default values** in data-carrying constructors (requests, payloads, domain models).
14. **Imports**, never fully-qualified types inline.
15. **Comments** — minimal is **mandatory**. Default to none. Write one ONLY where the code is genuinely specific and a reader could misread the logic without it (non-obvious **why**: invariant, workaround, platform quirk). Never restate what the code does. KDoc on `api/` public methods only.

When a more specific rule conflicts with one above (rare), the docs win — they have the rationale.

## Skills

User-invocable skills are packaged as the `polkadotapp-workflows` plugin under `.claude/plugins/polkadotapp-workflows/skills/`:
- `architect/SKILL.md` — design plan
- `implementer/SKILL.md` — write code
- `reviewer/SKILL.md` — audit a diff
- `commit-message/SKILL.md` — commit message formatting

Enable once per machine (from the repo root): `/plugin marketplace add ./.claude/plugins` then `/plugin install polkadotapp-workflows@polkadotapp-local`. The leading `./` matters — without it, Claude Code treats the argument as a GitHub `owner/repo` ref.
