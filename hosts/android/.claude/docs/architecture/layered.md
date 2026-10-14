# Layered Architecture (Data / Domain / Presentation)

Every feature module splits internally into three layers. Each layer owns its model types; crossings happen through explicit mappers.

## Rules at a glance

1. **`blocking`** — Domain layer importing Compose / Android UI / Room types — forbidden.
2. **`major`** — ViewModel calling a repository directly. Should go through an interactor (one per VM is the convention).
3. **`major`** — Room entity leaking out of the data layer.
4. **`major`** — Domain model carrying presentation fields (`@StringRes Int`, `isLoading`, `isPressed`).
5. **`major`** — DTO → domain mapping missing or located outside `feature/X/impl/data/mappers/`.
6. **`minor`** — Mapper as a heavyweight class when a pure extension function would suffice (or vice versa — extension function when injection is needed).
7. **`major`** — Single-line passthrough `UseCase` that just wraps a repo call — inline at the caller.
8. **`minor`** — UseCase introduced when only one feature needs the orchestration — make it an interactor instead.

## The layers

```
feature/<name>/
├── api/
│   ├── data/        ← repository interfaces, type registries
│   ├── domain/      ← cross-feature use cases, domain models, sealed errors
│   └── presentation/ ← public Mixin interfaces, navigation contracts
└── impl/
    ├── data/        ← repository impls, network DTOs (suffix `Remote`), mappers, DAOs
    ├── domain/      ← interactors (per-screen), feature-private use cases
    └── presentation/← ViewModels, Contract interfaces, Compose screens
        └── di/      ← Hilt modules
```

## Layer responsibilities & ownership

### Data
- Repositories — implement `XxxRepository` interfaces from `api/data/`.
- Network DTOs — suffix `Remote`, decorated with `@Keep` for Proguard.
- Local entities — Room types live in `database/`; per-feature read models stay here.
- **Always return `Result<T>`** for fallible IO. Never throw.
- Mappers — extension functions `fun XxxRemote.toDomain(): Xxx` in `impl/data/mappers/`.

### Domain
- Domain models — pure Kotlin, no Android/Compose/serialization annotations on the model class itself (kotlinx-serialization SCALE annotations are an exception when the model also lives on the wire — but keep `@Parcelize` off domain).
- Interactors — feature-private, **one per screen/VM**, mirror the presentation folder name (`domain/pairRequest/PairRequestInteractor.kt` for `presentation/pairRequest/`).
- Use cases — reusable across features. Live in `api/domain/usecase/` of the owning feature.
- Sealed `XxxError` types — see `code/results-and-errors.md`.

### Presentation
- ViewModels with `Contract` interfaces (see `code/state-management.md`).
- Compose screens (see `code/ui-compose.md`).
- **No business logic** here. ViewModels orchestrate; interactors do the work.
- UI state types — distinct from domain models when extra presentation-only fields are needed (formatted strings, selected, pressed). When the domain model is already pure Kotlin and the UI doesn't need extra fields, the VM may emit the domain model directly.

## Mapping rules

| Boundary | Mapping is | Lives in |
|---|---|---|
| DTO ↔ domain | **Mandatory.** Always map at the data layer. | `impl/data/mappers/` |
| Domain ↔ UI state | **Optional.** Map only when UI adds fields (selected, formatted, isLoading, etc.) | `impl/presentation/<screen>/mapper/` if non-trivial, else inline in VM |
| Domain ↔ database entity | **Mandatory.** Room entities never escape the data layer. | `impl/data/mappers/` |

✓ **Extension function form** for pure mappings:
```kotlin
// feature/sso/impl/data/mappers/HostMetadataMapper.kt
fun HostMetadataRemote.toDomain(): HostMetadata = HostMetadata(
    name = name,
    icon = icon,
)
```

✓ **Mapper class** when injection is needed:
```kotlin
class AccountMapper @Inject constructor(
    private val chainRegistry: ChainRegistry,
) {
    suspend fun toDomain(remote: AccountRemote): Account = ...
}
```

✗ Don't put presentation concerns in a domain model:
```kotlin
// ✗ Bad
data class HostMetadata(
    val name: String,
    val icon: Url,
    val isLoading: Boolean,            // presentation concern
    @StringRes val titleRes: Int,      // presentation concern
)
```

## Interactor vs UseCase

| | Interactor | UseCase |
|---|---|---|
| Scope | Single screen / VM | Cross-feature reusable |
| Lives in | `impl/domain/<screenName>/` | `api/domain/usecase/` (or `impl/domain/usecase/` if private) |
| Naming | `XxxInteractor` | `XxxUseCase` interface + `RealXxxUseCase` impl |
| Created… | **Always**, one per VM. Conventional layering. | When 2+ features need the same orchestration AND it's non-trivial. |
| Don't create when | (rule says always create) | Wrapping a single repo call — inline it. |

```kotlin
// ✓ Interactor — per-screen orchestration
class PairRequestInteractor @Inject constructor(
    private val repository: SsoHandshakeRepository,
    private val ssoHandshakeUseCase: SsoHandshakeUseCase,
) {
    suspend fun fetchHostMetadata(url: String): Result<HostMetadata> = ...
    suspend fun approveHandshake(offer: HandshakeOffer, metadata: HostMetadata): Result<Unit> =
        ssoHandshakeUseCase.respondToHandshake(offer).flatMap { saveSession(offer, metadata) }
}

// ✓ UseCase — non-trivial cross-feature logic
interface SsoHandshakeUseCase {
    suspend fun respondToHandshake(offer: HandshakeOffer): Result<Unit>
}
```

✗ Don't:
```kotlin
class IsPersonOnboardedUseCase @Inject constructor(private val statusUseCase: PersonStatusUseCase) {
    suspend operator fun invoke() = statusUseCase().map { it.isOnboarded }
    // Single-line passthrough — inline at the caller.
}
```

## ViewModel position in the layering

ViewModels are responsible for:
- Calling interactors (never repositories directly — those belong to the interactor).
- Producing the screen's `LoadingState<UiState>` / `XxxUiState` stream — combining the flows the interactor exposes into the final shape the screen consumes.
- Handing the screen the right structure for it to render: domain models passed through when no extra fields needed, sealed `XxxError` types so the Compose mapper resolves them to `stringResource(...)`.

**Flow composition is not VM-exclusive.** Interactors and use cases routinely `combine` / `map` / `flatMapLatest` over the streams their dependencies expose — that's how domain orchestration is built. The VM's job is the *outermost* composition that ends in `stateIn` for the screen. If the same composition recurs across two VMs, it belongs in an interactor (single-feature) or a use case (cross-feature), not duplicated.

```kotlin
// ✓ Interactor composing flows — produces the abstraction the VM needs
class PairRequestInteractor @Inject constructor(
    private val repository: SsoHandshakeRepository,
    private val sessionStore: SessionStore,
) {
    fun observePairRequestState(payload: PairRequestPayload): Flow<Result<PairRequestState>> =
        combine(
            repository.fetchHostMetadata(payload.url),
            sessionStore.observeForUrl(payload.url),
        ) { metadata, existingSession ->
            metadata.map { PairRequestState(it, existingSession) }
        }
}

// VM does the outer composition + stateIn
override val state = combine(
    interactor.observePairRequestState(payload),
    approving,
) { stateResult, isApproving -> stateResult.map { it.toUi(isApproving) } }
    .withLoading("PairRequest")
    .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)
```

ViewModels are **not** the place to:
- Construct domain entities for direct chain submission (use an interactor).
- Format dates/numbers/balances (use a mapper or extension).
- Hold mutable state that any other class also reads — that's a shared state holder, not VM state.
- Reach for repositories directly bypassing the interactor.

## Canonical examples

- **Interactor + VM + UseCase done well**: `feature/sso/impl/.../pairRequest/PairRequestInteractor.kt` + `PairRequestViewModel.kt` + `SsoHandshakeUseCase`.
- **Mapper pattern**: `feature/sso/impl/data/mappers/`.
- **Sealed domain error → UI resource**: see `code/results-and-errors.md`.
