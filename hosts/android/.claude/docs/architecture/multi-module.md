# Multi-Module Architecture

## Rules at a glance

1. **`blocking`** — `feature/X/impl` never depends on another `feature/Y/impl`. Always go through `api`.
2. **`blocking`** — `chains`, `database`, `common`, `design` never depend on a `feature/*` or `app/`.
3. **`blocking`** — `design` does not depend on `common` either.
4. **`blocking`** — New module added with **no** entry in `CODEOWNERS` (no reviewer for the module).
5. **`major`** — Logical cycle: `feature/A/api` depends on `feature/B/api` AND `feature/B/impl` depends on `feature/A/api`. Extract the shared concept into a more general module (precedent: ring-VRF → `feature/members`).
6. **`major`** — New module added without a `CODEOWNERS` update in the same PR.
7. **`minor`** — New module added with only one owner in `CODEOWNERS`. The agent cannot pick the right co-owner; surface to the human author.
7. **`major`** — Cross-feature wiring done via one-off direct injection instead of `@IntoSet` / `@IntoMap` multibinding.
8. **`major`** — Feature-specific entity placed in shared `database` without a feature-prefixed name.
9. **`minor`** — Package leaves lowercase-glued (`pairrequest`) instead of `camelCase` (`pairRequest`).

## Purpose

PolkadotApp is split across **35+ feature modules** and **15+ tool modules**, each with an `api/` and `impl/` sub-module. This split exists so that features can be assembled freely by `app/` without depending on each other's implementation details. Boundaries that look harmless from a Gradle compile perspective can still introduce logical coupling that defeats the purpose — see "Logical cycles" below.

## The module map

```
.
├── app/                    ← entry point; depends on every feature's impl
├── common/                 ← shared utilities; depends on design only
├── design/                 ← pure Compose design system; depends on nothing app-specific
├── database/               ← Room; depends on common
├── chains/                 ← chain registry, runtime metadata, RPC; depends on common, database
├── feature/<name>/api      ← public contracts: interfaces, data classes, domain models
├── feature/<name>/impl     ← implementations + Compose UI + DI bindings
├── tools/<name>/api        ← cross-cutting tool contracts
├── tools/<name>/impl       ← tool implementations
├── bindings/<name>         ← Rust FFI (Bandersnatch, HydraDX math)
└── test-shared/            ← test fixtures
```

## Dependency rules

### ✓ Allowed

| From | May depend on |
|---|---|
| `app/` | everything |
| `feature/X/impl` | `feature/X/api`, any `feature/Y/api`, `common`, `design`, `chains`, `database`, `tools/Z/api` |
| `feature/X/api` | `common`, `chains`, other `feature/Y/api` (sparingly), `tools/Z/api` |
| `tools/X/impl` | `tools/X/api`, other `tools/Y/api`, `common`, `design`, `chains`, `database` |
| `common` | `design` |
| `chains` | `common`, `database`, `tools/remoteconfig/api` |
| `database` | `common` |
| `design` | (nothing app-specific) |

### ✗ Forbidden — hard rules

- **`impl` never depends on another `impl`.** Always go through `api`.
- **`chains`, `database`, `common`, `design` never depend on `feature/*` or `app/`.**
- **`design` never depends on `common` either** — keeps the design system reusable in isolation.
- **`feature/X/api` never depends on `feature/X/impl`** — obvious, but worth saying.

### ⚠ Logical cycles — the subtler trap

Beyond Gradle cycles, watch for **logical cycles**: a graph that compiles but creates round-trip coupling.

**Example of a logical cycle:**
> `feature/statement-store/api` depends on `feature/products/api`,
> and `feature/products/impl` depends on `feature/statement-store/api`.

Not a Gradle cycle — but the two features are interlocked. Changes in one ripple both directions.

**When you hit one:** the right move is to **extract** the shared concept into a more general module, not to add a "small dep that should be fine".

**Precedent**: ring-VRF proofs were extracted from `feature/people` into `feature/members` precisely for this reason.

## Naming

- **Module name**: `feature/<kebab-case>/<api|impl>` or `tools/<kebab-case>/<api|impl>`.
- **Package**: `io.paritytech.polkadotapp.feature_<name>_<api|impl>.<layer>.<area>`. Example: `io.paritytech.polkadotapp.feature_transfers_impl.data.repository`.
- **Package leaves** are **camelCase**, never lowercase-glued: `pairRequest`, not `pairrequest`.
- **API binding class**: `Real<InterfaceName>` (e.g. `RealSendRecipientRepository`).

## DI

- **Hilt** with `SingletonComponent` for most bindings. Activity/View/Fragment-scoped bindings live in the matching component.
- Each `impl` module has `di/<Feature>FeatureModule.kt` providing `@Binds` from `Real*` → public interface.
- Cross-feature wiring goes through `@IntoSet` / `@IntoMap` multibindings (e.g. `Set<ChatExtension>`, `Set<AppInitializer>`).

```kotlin
// ✓ feature/transfers/impl/.../di/TransfersFeatureModule.kt
@Module
@InstallIn(SingletonComponent::class)
interface TransfersFeatureModule {
    @Binds @Singleton
    fun bindSendRecipientRepository(impl: RealSendRecipientRepository): SendRecipientRepository
}
```

## Settings & build conventions

- `settings.gradle.kts` lists every module. Adding one means: create the folder, register here, give it a `build.gradle.kts` matching the convention.
- A new feature should be scaffolded with `developer-tools/create_module.kts` (do not hand-roll).
- `impl` modules apply `dagger.hilt`, `kotlin.compose` (when UI is present), `kotlin.parcelize` (when nav args), `kotlin.serialization` (when SCALE).

## CODEOWNERS — mandatory when adding a module

When a new module is added (`feature/<name>/` or `tools/<name>/`), the PR author **must** update `CODEOWNERS` in the same PR to list:

1. **Themselves** as an owner.
2. **Anyone who participated** in the design / RFC / pre-implementation discussion of the feature.
3. **Anyone who knows the surrounding context** even if they didn't write the code.

The goal: **no single-owner modules.** A module owned by one person is a single point of failure for reviews and for institutional knowledge.

```
# CODEOWNERS
/feature/coinage/         @valentunn @foxwoosh @<feature-discussion-participants>
/feature/<new-feature>/   @<author> @<co-reviewers>
```

**Agents (architect / implementer) must NOT pick the second owner.** Co-ownership is a human decision driven by who participated in the design / who knows the surrounding context. The agent does not have that context; guessing inserts the wrong name and creates churn. When a new module is in scope, the implementer **stops and asks** the human author who the co-owner(s) should be before writing the CODEOWNERS line. If the author can't name one yet, ship with the author alone listed — the reviewer will surface this as a `minor` follow-up.

Reviewer severity:
- **blocking** — new module's PR doesn't touch `CODEOWNERS` at all (no reviewer for the module).
- **minor** — `CODEOWNERS` lists only one owner. The agent surfaces this for the human author to resolve; doesn't block merge.

## Where new things live

| Concept | Goes in |
|---|---|
| New domain model used by 2+ features | `feature/<owner>/api/domain/model/` |
| Network DTO | `feature/<owner>/impl/data/network/` (suffixed `Remote`) |
| Mapper DTO → domain | `feature/<owner>/impl/data/mappers/` as `fun XxxRemote.toDomain(): Xxx` |
| UseCase reused across features | `feature/<owner>/api/domain/usecase/` |
| Interactor for a single screen | `feature/<owner>/impl/domain/<screenName>/` |
| Reusable VM Mixin (e.g. AmountInput, Fee) | `feature/<owner>/api/presentation/mixin/` (interface + Factory), `impl/presentation/mixin/` (Real binding) |
| Router interface | `feature/<owner>/api/.../<Owner>Router.kt` |
| Navigator implementation | `app/root/navigation/<owner>/<Owner>Navigator.kt` |
| Background worker | `feature/<owner>/impl/.../data/worker/` |
| New Nova widget | `design/src/.../components/<category>/Nova<Name>.kt` |
| Shared icon (24x24) | `design/src/.../components/icon/vectors/` |
| Feature-specific icon | `feature/<X>/impl/.../presentation/compose/components/icons/` |
| Shared string | `common/src/main/res/values/strings.xml` |
| Database entity / DAO | `database/src/main/.../entity/` and `dao/` |
| Chain RPC / runtime call wrappers | `chains/src/main/.../<area>/` |

## Canonical examples to imitate

- Clean feature split: `feature/transfers/` (clear api/impl, mappers, interactor, mixin via `PreviousPaymentsAddressConverterFactory`).
- Tool example: `tools/auth/` (api binds `AuthService`, impl wraps OAuth provider).

---

## Multi-feature pipeline collection (Compound + multibinding)

When a "hub" feature needs to **gather contributions from N other features it doesn't know about**, use the *Compound-with-multibinding* pattern. This keeps the hub fully decoupled from contributors while still letting it enumerate everything they offer.

**Example shape** — a `Scan` feature that wants to dispatch a scanned QR / deep link to whichever feature owns that domain (wallet, transfer, etc.):

### Step 1 — Hub defines the contributor interface in its `api`

```kotlin
// feature/scan/api/.../DeeplinkScanner.kt
interface DeeplinkScanner {
    suspend fun tryHandle(payload: ScanPayload): ScanResult
}
```

### Step 2 — Hub defines a `Compound*` that injects the multibinding set

```kotlin
// feature/scan/impl/.../CompoundDeeplinkScanner.kt
@Singleton
class CompoundDeeplinkScanner @Inject constructor(
    private val scanners: Set<@JvmSuppressWildcards DeeplinkScanner>,
) : DeeplinkScanner {
    override suspend fun tryHandle(payload: ScanPayload): ScanResult {
        scanners.forEach { scanner ->
            val result = scanner.tryHandle(payload)
            if (result is ScanResult.Handled) return result
        }
        return ScanResult.Unhandled
    }
}
```

The hub's other components (VMs, services) depend on `DeeplinkScanner` (interface) and Hilt binds them to `CompoundDeeplinkScanner`. They never see the individual contributors.

### Step 3 — Each contributing feature implements + binds itself into the set

```kotlin
// feature/wallet/impl/.../WalletDeeplinkScanner.kt
class WalletDeeplinkScanner @Inject constructor(
    private val router: WalletRouter,
) : DeeplinkScanner {
    override suspend fun tryHandle(payload: ScanPayload): ScanResult = when {
        payload.matchesWalletScheme() -> { router.open(...); ScanResult.Handled }
        else -> ScanResult.Unhandled
    }
}

// feature/wallet/impl/.../di/WalletModule.kt
@Module @InstallIn(SingletonComponent::class)
interface WalletModule {
    @Binds @IntoSet
    fun bindWalletDeeplinkScanner(impl: WalletDeeplinkScanner): DeeplinkScanner
}
```

### Why this pattern

- **Hub has zero compile-time knowledge of contributors.** Adding a new feature that wants to participate is a one-line `@Binds @IntoSet` in *that feature*; no touch to the hub.
- **Avoids reverse dependencies.** Without this pattern, `feature/scan/impl` would need to depend on every feature that handles a deep link — exactly the kind of fan-out the `impl→api` rule and the "no logical cycles" rule (see § Logical cycles) prohibit.
- **Composition shape is uniform.** Any "gather from N features" need uses the same triplet: `Xxx` interface in hub api, `CompoundXxx` in hub impl, `@IntoSet` in contributing impls.

### When NOT to use it

- One specific feature owns the concern; no other feature contributes. Use a direct binding.
- Order matters in a way the multibinding `Set` can't express. Use `@IntoMap` keyed by an enum/priority, or model the ordering domain explicitly.

### Existing examples in the codebase

- `Set<@JvmSuppressWildcards ChatExtension>` — chat extensions such as `CoinagePaymentProcessingExtension` and `SampleBot` (`feature/chats/impl/.../domain/middleware/ChatExtensionRegistry.kt`).
- `Set<@JvmSuppressWildcards AppInitializer>` — startup work contributed by features (`common/.../presentation/AppInitializerPipeline.kt`).
