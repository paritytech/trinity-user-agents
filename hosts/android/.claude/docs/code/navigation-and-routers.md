# Navigation and Routers

Every feature exposes a **`<Feature>Router`** interface in its `api/` module that the rest of the feature (ViewModels, screens, sheets) calls to navigate. The interface is implemented by a **`<Feature>Navigator`** in `app/` that knows about the global navigation graph, bound by Hilt as the only place that touches `NavController`.

This split keeps Jetpack-Navigation knowledge out of feature modules and avoids `app → feature/impl` reverse deps.

---

## The shape

### Router interface — lives in feature `api/`

```kotlin
// feature/<X>/api/.../<X>Router.kt
interface FooRouter : ReturnableRouter {
    fun openDetails(payload: FooPayload)
    fun openSettings()
}
```

- Methods are **semantic intents**, not generic (`openDetails`, not `navigate`).
- Common bases:
  - **`ReturnableRouter`** — `back()`, `backWithResult(key, value)`.
  - **`TabRouter`** — `openWalletTab()`, `openChatsTab()`, `openSettingsTab()`.
  - Domain mix-ins where reused, e.g. `SigningRouter` for signing flows shared across features.
- Routers may extend more than one base: `interface ChatsRouter : ReturnableRouter, TabRouter`.

ViewModels and screens consume the interface. They have **no** dependency on `NavController`, fragment IDs, or `app/`.

### Navigator implementation — lives in `app/`

```kotlin
// app/root/navigation/foo/FooNavigator.kt
class FooNavigator @Inject constructor(
    private val navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), FooRouter {

    override fun openDetails(payload: FooPayload) {
        performNavigation(
            R.id.action_fooFragment_to_fooDetailsFragment,
            args = FooDetailsFragment.bundleOf(payload),
        )
    }

    override fun openSettings() = performNavigation(
        cases = arrayOf(
            R.id.fooFragment to R.id.action_fooFragment_to_settingsFragment,
            R.id.fooEmptyFragment to R.id.action_fooEmptyFragment_to_settingsFragment,
        ),
    )
}
```

Key helpers from `BaseNavigator` (`app/root/navigation/BaseNavigator.kt`):
- `performNavigation(@IdRes actionId, args?, navOptions?)` — straight action dispatch.
- `performNavigation(cases: Array<Pair<Int, Int>>, ...)` — **conditional** navigation when the current destination can be one of several.
- `back()` — pop the back stack.
- `backWithResult(key, result)` — Jetpack-Nav set-result via the previous entry's `SavedStateHandle`.

### `NavigationHolder` — the only global

```kotlin
// app/root/navigation/NavigationHolder.kt
class NavigationHolder {
    var navController: NavController? = null
    var openBottomTabListener: ((tabId: Int) -> Unit)? = null

    fun executeBack() { navController?.popBackStack() }
    fun executeBackWithoutResult() { navController?.popBackStack() }
}
```

The host activity (`RootActivity`) registers its `NavController` with the holder; all navigators read it through this single surface.

### DI wiring

```kotlin
// app/root/navigation/NavigatorsModule.kt
@Module @InstallIn(SingletonComponent::class)
interface NavigatorsModule {

    @Binds
    @Singleton
    fun bindFooRouter(impl: FooNavigator): FooRouter
}
```

The router interface is the binding target. Feature code injects `FooRouter`; Hilt provides the `FooNavigator` impl. Singletons are appropriate here because the holder is also singleton.

---

## Rules

1. **Router interface in `api/`**, navigator class in `app/`. Never the other way around.
2. **Semantic method names** — `openContactDetails(contactId)`, not `navigate(...)` or `handleAction(...)`.
3. **No `NavController` in feature code.** If a feature ViewModel needs to navigate, it depends on its router. Period.
4. **No payload primitives** — pass `Parcelable` or `@Serializable` payload classes through bundles, not loose strings/ints. `@Parcelize` belongs in the presentation layer (the navigation arg type), not on domain models.
5. **Tab navigation** uses `TabRouter` and goes through `openBottomTabListener` set up by `RootActivity` — not a `performNavigation`.
6. **One navigator per feature**, even when the feature spans multiple graphs. Conditional `performNavigation(cases = ...)` handles multi-origin actions.
7. **Result delivery** — use `backWithResult(key, result)` + `SavedStateHandle.getStateFlow(key)` in the caller's VM. Don't reach for a shared singleton or pass a callback through navigation args.
8. **Signing / Scan / Reusable flows** — when multiple features need the same sub-flow (signing, QR scan, address picker), expose a small mix-in interface in `api` (`SigningRouter`, `ScanRouter`) and have features' routers inherit it.

---

## Where new things live

| Concept | Goes in |
|---|---|
| New router interface | `feature/<X>/api/.../<X>Router.kt` |
| Router method | added to the interface in `api`; implemented in `app/.../navigation/<x>/<X>Navigator.kt` |
| Cross-feature router mix-in | `feature/<X>/api/.../<X>Router.kt` as a sibling interface (e.g. `SigningRouter`) |
| Navigation graph entry / action | XML under `app/src/main/res/navigation/` |
| Payload type for navigation args | `feature/<X>/api/.../presentation/.../<X>Payload.kt` with `@Parcelize` |

---

## ✗/✓ examples

### ✗ ViewModel depending on `NavController`

```kotlin
class FooViewModel @Inject constructor(private val navController: NavController) : BaseViewModel() {
    fun onCardClicked() {
        navController.navigate(R.id.fooDetailsFragment)
    }
}
```

ViewModel reaches into navigation infra. `NavController` is Activity-scoped, never re-bindable, and lives in `app/`.

### ✓ ViewModel depending on its Router

```kotlin
class FooViewModel @Inject constructor(private val router: FooRouter) : BaseViewModel() {
    fun onCardClicked() {
        router.openDetails(currentPayload)
    }
}
```

### ✗ Generic `navigate` method

```kotlin
interface FooRouter {
    fun navigate(action: NavAction)  // — opaque, loses semantic intent
}
```

### ✓ Semantic intents

```kotlin
interface FooRouter : ReturnableRouter {
    fun openDetails(payload: FooPayload)
    fun openSettingsFromList()
    fun openSettingsFromEmpty()
}
```

### ✗ Passing primitives instead of payload class

```kotlin
fun openDetails(id: String, name: String, deepLink: String?)
```

### ✓ Payload class

```kotlin
fun openDetails(payload: FooDetailsPayload)
```

### ✗ Reverse dep: navigator in feature `impl`

```kotlin
// feature/foo/impl/.../FooNavigator.kt  ← wrong
class FooNavigator(private val navHolder: NavigationHolder) : FooRouter
```

Pulls Jetpack-Navigation knowledge into the feature module, breaks the `app → impl` direction, and prevents `app/` from owning the routes.

### ✓ Navigator in `app/`

```kotlin
// app/root/navigation/foo/FooNavigator.kt
class FooNavigator @Inject constructor(...) : BaseNavigator(...), FooRouter
```

---

## Anti-patterns

- **Mixing router and use-case responsibilities** — `FooRouter.submitAndNavigate(...)` is wrong. Routers navigate; ViewModels orchestrate.
- **Hardcoded `Fragment` class names inside the feature.** The feature should never know `FooFragment` exists; it knows `FooRouter.openFoo()`.
- **Singleton routers reaching out for side-effects** — routers should be thin shims. Anything more than a `performNavigation` call belongs in a ViewModel or interactor.
- **Returning data from `router.openX()`** — navigation is fire-and-forget. Data comes back via `backWithResult` + `SavedStateHandle`.
- **Reaching into another feature's router** — features depend on their own router only. If a feature needs to launch another feature, the necessary entry point belongs in a shared mix-in (`SigningRouter`, `ScanRouter`) that both feature routers extend.

---

## Reviewer flags

- **major** — VM/Compose/Interactor depending on `NavController`, `Fragment`, or `app/` types.
- **major** — Router in `feature/<X>/impl/` instead of `api/`; Navigator outside `app/`.
- **major** — Router method named generically (`navigate`, `handleAction`).
- **major** — Loose primitives in router method signatures where a payload class is appropriate.
- **minor** — Router method that does more than navigate (state mutation, suspend with business logic).

---

## Canonical examples

- Standard router + navigator pair: `WalletRouter` / `WalletNavigator`.
- Multi-base router: `ChatsRouter : ReturnableRouter, TabRouter`.
- Cross-feature mix-in: `SigningRouter` (in `feature/products/api`) consumed by `ProductsRouter`.
- Conditional navigation: any navigator using `performNavigation(cases = ...)`.
- Result delivery: `backWithResult(key, result)` callers in scan / address picker flows.
