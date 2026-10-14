# Maintainability Principles

Cross-cutting principles that hold across every module and layer. Each rule includes the failure mode it prevents and a pointer to a real PR where it was litigated.

## Rules at a glance

1. **`major`** — Single concern per class. Two unrelated concerns (load + decide + render) must be separated.
2. **`major`** — Domain identifiers use `@JvmInline value class` (when invariants apply) or `typealias` (when only naming applies). Primitives only for trivial values.
3. **`major`** — Composition over inheritance. New hierarchies need exhaustive-matching or shared-finalize justification.
4. **`major`** — Constructor injection over implicit cross-dependencies via globals (no init-block side effects via Dagger).
5. **`major`** — No factory soup. Single `Factory.create(scope, config)` over factory-of-factory chains.
6. **`major`** — Don't paper symptoms — fix root causes (`AlwaysFirst` over `PinToTop`; `synthesizeSnapshot`).
7. **`major`** — No leaky abstractions. Style params honored; renderers don't smuggle Fragments.
8. **`major`** — Feature boundaries are real — `impl` is private; shared types go in the most general module that doesn't pull in unrelated concerns.
9. **`major`** — When the codebase is mid-migration, new code points at the north star even when shipping current-state idioms.
10. **`major`** — Reuse before you build. Check `design/`, `common/utils/`, the doc tree before introducing a new primitive.

## 1. Single concern per class

A class has **one reason to change**. Its public API should describe one job.

- ✗ `SessionManager` knowing about specific custom message types — violates SRP because adding a new message type forces a change in `SessionManager`. Fix: expose a generic subscription API.
- ✗ `ConfettiSoundPlayer` *both* setting up MediaPlayer *and* deciding when to play. Fix: VM decides when; player just plays.

When a class grows past one job, split. When two jobs always travel together, the abstraction is wrong — find the unifying concept or accept the split.

## 2. Maintain invariants — narrow types over primitives

Domain identifiers and bounded values should be modeled with the strongest type that still pays for itself:

| Use… | When |
|---|---|
| `@JvmInline value class` with **private constructor + companion factory** (`fromUrl`, `fromBytes`, `fromUserInput`) | An invariant must hold. The type cannot be constructed from arbitrary input. |
| `typealias` | A new domain meaning is useful but no invariant is enforced. |
| Raw primitive | Truly trivial values (loop indices, counts). |

```kotlin
// ✓ Value class with invariant
@JvmInline
value class ProductId private constructor(val value: String) {
    companion object {
        fun fromUrl(uri: Uri): ProductId? = uri.host?.takeIf { it.endsWith(".dot") }?.let(::ProductId)
        fun fromLocalId(id: String): ProductId = ProductId(id) // internal use
    }
}

// ✓ Typealias — new domain meaning, no invariant
typealias ChainId = String

// ✗ Bare String for a domain identifier with rules
fun openProduct(productId: String) // — caller can pass anything
```

## 3. Composition over inheritance

Default to composition. Inheritance has narrow legitimate uses:
- **Sealed hierarchies** that need exhaustive `when` matching.
- A truly shared lifecycle hook that derived classes finalize and add to (e.g. `ChatBot` finalizing `ChatExtension` to add per-bot room creation).

✗ Don't:
- Build a hierarchy of `BaseScreen` / `BaseFragment` / `BaseViewModel` chained inheritance.
- Add an `open` method on a leaf class "in case" subclasses want to override.

## 4. Clear API; no implicit cross-dependencies

If a class needs something, it gets it via constructor. Two classes don't communicate through a global state holder unless that state holder is itself a documented abstraction (e.g. `ChatBotStateController`).

- ✗ Injecting a class only to trigger Dagger to construct it so its `init {}` block runs side effects. Fix: use `AppInitializer` + `AppInitializerPipeline` (already in code: `common/.../presentation/AppInitializer.kt`).
- ✗ A VM reaching for `Activity` to fire side effects. Fix: emit a one-shot command to the screen which dispatches.
- ✗ A repository handling persistence directly via `Preferences`. Fix: typed `XxxStorage` interface, repository depends on the typed storage.

## 5. No factory soup

When you have a factory whose only job is to create another factory, the abstraction is wrong. Question every layer of indirection: does it earn its keep?

- ✗ "Factory creates Factory creates Builder creates Provider".
- ✓ Single `Factory.create(scope, config)` returns the working instance.

## 6. Don't paper symptoms — fix root causes

When a workaround surfaces (extra flag, escape-hatch method, conditional special case), step back. Usually the root model is wrong.

- ✗ Adding `AlwaysFirst` ordering "above the top" because `PinToTop` ties with multiple bots. Fix the ordering model, don't stack flags.
- ✗ `synthesizeSnapshot()` because the snapshot is now needed outside service lifetime. Fix: make the snapshot a global state holder regardless of service.

## 7. No leaky abstractions

A class advertises behavior X; do that. Don't take secret detours.

- ✗ A `style: ChatMessageSurfaceStyle` parameter that's silently overridden by an ambient `LocalGlassMessageBubbles`. Caller passing a style has a right to expect it's respected.
- ✗ A `ChatExtension` smuggling Fragments into composable UI. Encapsulate; expose Compose API only.

## 8. Boundaries are real — features don't reach into other features' privates

- A feature's `impl` module **must not** be referenced by another feature.
- A feature's public types **must not** leak into `common`/`design`/`database` (review example: feature-specific entity in common DB → required feature prefix such as `Coinage`, or move).
- An RFC-defined cross-app concern (e.g. chat protocol) lives in the smallest module that everyone agrees on, not in whatever feature first needed it.

## 9. The "north star" exists

When the codebase is mid-migration toward a new design (chat extension v1 → RFC-0002 v2; product handlers; allowance/PGAS), new code points in the v2 direction even when it currently has to ship v1 idioms. See `chat-extension.md § North star` and `host-api-products.md`.

If a change cannot align with the north star, name that explicitly in the plan — don't quietly cement the v1 shape.

## 10. Reuse before you build

Before introducing a new widget/utility/abstraction, check what exists:
- UI primitives → `design/` (see `code/ui-compose.md` for catalog).
- Result/Flow extensions → `common/utils/Result.kt`, `common/utils/Flows.kt`.
- Loading state → `LoadingState<T>` + `.withLoading("Tag")`.
- Storage → typed `XxxStorage` interfaces, not bare `Preferences`.
- HTTP — Retrofit + `NetworkApiCreator`; don't duplicate base URLs.

When reuse genuinely doesn't fit, *that's* when you build new — and you build it in the most general module that doesn't pull in unrelated concerns.

## Checklist for the architect

For any non-trivial change, the plan should be able to answer:

- [ ] What is the **one** concern of each new class?
- [ ] Does every new identifier type have either an invariant (value class) or just a typealias?
- [ ] Is there any new inheritance? If so, justified by sealed-matching or shared finalize hook?
- [ ] Could two classes communicate via constructor injection instead of any global?
- [ ] How many factory layers? Drop one if possible.
- [ ] Is this a workaround for a deeper modeling problem?
- [ ] Does it cross a module boundary it shouldn't?
- [ ] Does it move toward the north star, or away?
- [ ] What exists already that I'm duplicating?
