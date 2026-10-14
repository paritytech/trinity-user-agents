# Compose UI

Built on Nova-prefixed wrappers over Material3. Every screen follows the public-screen / internal-screen / preview triplet. No raw Material widgets, no inlined hard-coded strings/colors, no Modifier mutation, no early returns in `@Composable`.

## Rules at a glance

1. **`major`** — Use `Nova*` wrappers in place of raw Material widgets. Full catalog: `code/nova-widget-inventory.md`.
2. **`major`** — `Modifier` is the **first** parameter. Never mutate a passed-in modifier — apply on the caller side or build the whole modifier internally.
3. **`major`** — No early returns inside a `@Composable`. Wrap in `if`/`when`.
4. **`major`** — `VerticalSpacer { spacingN }` / `HorizontalSpacer { spacingN }`. Never `Spacer(Modifier.height(...))`.
5. **`major`** — `NovaTheme.spacings` is for paddings and margins **only**. Never for radii, sizes, or stroke widths.
6. **`major`** — `NovaSurface` for any widget needing background / shape / border / elevation / ripple. Don't reach for raw `Box(.background.clip.border)`.
7. **`major`** — Strings always via `stringResource(RCommon.string.…)`. No hardcoded UI strings; no `@StringRes Int` resolved in the ViewModel.
8. **`major`** — Colors via `NovaTheme.colors.*`. No `Color(0xFF…)` / `Color.Black` at the feature layer.
9. **`major`** — `BottomSheets` / `AlertDialogs` live in the **public** screen, not the internal screen.
10. **`major`** — `BackHandler` lives in the screen, not the Fragment.
11. **`major`** — `clickable(enabled = ...)`, not `if (enabled) Modifier.clickable {} else this`.
12. **`major`** — `reverseLayout = true` always in chat feeds.
13. **`major`** — Use `LazyListScope.animateItem()`; no custom item-entry animations.
14. **`major`** — Custom `Canvas` only when an `ImageVector` / SVG genuinely cannot express the shape, or per-frame draw is required.
15. **`major`** — `Modifier.blur` is API 31+; guard or polyfill.
16. **`major`** — Use `BaseComposeBottomSheet` when **any** of these is true: the sheet is its own navigation destination; the sheet has its own `BaseViewModel`; the sheet must survive Activity recreation; the sheet is launched from outside any screen. Otherwise use `NovaModalBottomSheet` in the public screen.
17. **`major`** — VM Contract / state does **not** expose raw Android framework widgets (`View`, `Bitmap`, `Drawable`, `Window`). Expose a domain-shaped state or a stable holder; the screen does the framework interop. **Exception (downgrade to `minor`)**: `WebView` handed to Compose via `AndroidView`, where preloading / off-tree warm-up forces a VM-owned instance. Prefer a thin `WebViewSlot`/`WebViewHolder` wrapper even there; flag the bare `StateFlow<WebView?>` as minor.
18. **`minor`** — Symmetric padding by default (top/bottom both `spacingN`); aids component reuse.
19. **`minor`** — Compose file > ~400 lines should split into `components/`.
20. **`minor`** — Odd pixel sizes (3.dp, 5.dp) will subpixel-jump on different DPIs.
21. **`major`** — UI-state collections are `ImmutableList` / `ImmutableSet` / `ImmutableMap`, **and** the element type is stable. `ImmutableList<T>` is unstable when `T` is unstable.
22. **`major`** — Presentation models held in Compose state are `data class` (value `equals`), never a plain `class` without `equals`.
23. **`major`** — Read frequently-changing `State` (scroll offset, animation/drag progress) at the lowest composable that uses it, and put raw derived reads behind `derivedStateOf` — never let a high-frequency read recompose a parent subtree. (Scroll example: `code/ui-compose.md § Recomposition and stability`.)
24. **`major`** — An effect's callback must not mutate state that re-feeds the same composable's inputs (recomposition loop) — e.g. a per-item `LaunchedEffect` updating the VM-backed list it iterates.

---

## Screen architecture

### File structure (per feature `Foo`)

```
feature/<X>/impl/.../presentation/foo/
├── FooContract.kt              ← interface; state, actions
├── FooViewModel.kt             ← @HiltViewModel implementing FooContract
└── compose/
    ├── FooScreen.kt            ← public screen + internal screen + preview
    └── components/             ← screen-local Composables
```

### Bottom sheets — `NovaModalBottomSheet` vs `BaseComposeBottomSheet`

Two bottom-sheet patterns coexist in the codebase; pick the right one by who owns the sheet's lifetime:

| Use… | When | Where it lives |
|---|---|---|
| **`NovaModalBottomSheet`** | The sheet is part of a screen — opened/closed by a screen-local state toggle, dismissed when the user leaves the screen | Inside the **public** screen (`FooScreen`), never inside the internal screen |
| **`BaseComposeBottomSheet<T : BaseViewModel>`** | The sheet is its own navigation destination, has its own `BaseViewModel`, must survive Activity recreation, or is launched from outside any screen | Subclass placed under `feature/<X>/impl/.../presentation/<flow>/<Name>BottomSheet.kt` |

#### `NovaModalBottomSheet` — the in-screen pattern

```kotlin
@Composable
fun FooScreen(contract: FooContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    var sheetVisible by remember { mutableStateOf(false) }

    FooScreenInternal(state = state, onShowSheet = { sheetVisible = true })

    NovaModalBottomSheet(isVisible = sheetVisible, onDismissRequest = { sheetVisible = false }) {
        SheetContent()
    }
}
```

Owned by Compose. No fragment, no DI scope, no `argument(...)` plumbing. Use this whenever the sheet is logically "part of the screen".

#### `BaseComposeBottomSheet` — the destination pattern

```kotlin
@AndroidEntryPoint
class FooBottomSheet : BaseComposeBottomSheet<FooViewModel>() {

    override val viewModel: FooViewModel by viewModels()

    @Composable
    override fun Screen() {
        FooSheetContent(contract = viewModel)
    }

    companion object {
        private const val KEY_PAYLOAD = "payload"

        fun bundleOf(payload: FooPayload): Bundle = bundleOf(KEY_PAYLOAD to payload)

        fun argument(args: Bundle): FooPayload = args[KEY_PAYLOAD] as FooPayload
    }
}
```

The base (in `common/.../presentation/screens/BaseComposeBottomSheet.kt`) handles:
- `NovaTheme(isRetroMode = ...)` wrapping with `ThemePreferences.retroModeFlow()`.
- `ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed` for safe Compose ↔ Fragment integration.
- `BottomSheetBehavior` configured with `skipCollapsed = true` and initial state `STATE_EXPANDED`.
- ViewModel-event subscription via `BaseFragmentDelegate`.

Rules:
- **One `BaseViewModel` per sheet**, injected via `@HiltViewModel`.
- **Args via `argument<T>(key)` helper** — typed bundle access, no manual casts at the call site.
- **Navigation graph entry** for the sheet under `app/src/main/res/navigation/` so the router can route to it.
- **Sheet content is its own `@Composable` function** consuming the contract — same triplet shape (public sheet / internal pure-state composable / preview).

#### When NOT to use `BaseComposeBottomSheet`

- The sheet has no domain state — it's a confirmation prompt or simple selector. Use `NovaAlertDialog` or in-screen `NovaModalBottomSheet`.
- The sheet is opened from inside another screen and dismissed when the screen leaves — same answer.

---

### Triplet inside `FooScreen.kt`

```kotlin
// 1. PUBLIC — takes Contract, collects state, hosts overlays (dialogs, sheets, permissions)
@Composable
fun FooScreen(contract: FooContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    var sheetVisible by remember { mutableStateOf(false) }

    FooScreenInternal(
        state = state,
        onPrimary = contract::onPrimaryClicked,
        onShowSheet = { sheetVisible = true },
    )

    NovaModalBottomSheet(isVisible = sheetVisible, onDismissRequest = { sheetVisible = false }) {
        SheetContent()
    }
}

// 2. INTERNAL — pure state + lambdas, contains the layout
@Composable
private fun FooScreenInternal(
    state: FooUiState,
    onPrimary: () -> Unit,
    onShowSheet: () -> Unit,
) { ... }

// 3. PREVIEW — at the bottom
@Preview
@Composable
private fun FooScreenPreview() {
    NovaTheme { FooScreenInternal(state = FooUiState(), onPrimary = {}, onShowSheet = {}) }
}
```

**Public screen** hosts dialogs, bottom sheets, toasts, permission requests. The internal screen never knows about overlays.

✗ Putting `NovaAlertDialog` / `NovaModalBottomSheet` inside the internal screen.

---

## Nova wrappers — always use these

**`major`** — Never use a raw Material widget when a Nova equivalent exists.

Most-used:

| Want | Use |
|---|---|
| Button | `NovaButton`, `NovaTextButton` |
| Text | `NovaText` |
| Text field | `NovaTextField` |
| Icon | `NovaIcon` |
| Container w/ background/border/shape/elevation | `NovaSurface` |
| Modal sheet | `NovaModalBottomSheet` |
| Alert dialog | `NovaAlertDialog` |
| Vertical/horizontal gap | `VerticalSpacer { spacingN }` / `HorizontalSpacer { spacingN }` |
| Empty state | `EmptyScreenState(title, message)` |
| Loading screen | `LoadingScreenState` |

**Full catalog** (avatars, dropdowns, top bars, progress variants, tooltips, async images, QR, mnemonic, shimmer, dialogs, error states, placeholders, etc.) lives in `code/nova-widget-inventory.md` — load that doc when you need to discover whether a Nova widget exists for what you're about to write. Don't auto-load it for every Compose edit.

---

## `NovaSurface` — the container primitive

```kotlin
// ✓
NovaSurface(
    modifier = Modifier.fillMaxWidth(),
    shape = RoundedCornerShape(12.dp),
    color = NovaTheme.colors.backgroundSecondary,
    border = NovaSurfaceBorder(...),       // ← function args, not Modifier
    onClick = { onClick() },                // ← function arg
) {
    // content
}

// ✗ Don't apply background/clip/border via Modifier
Box(modifier = Modifier
    .background(NovaTheme.colors.backgroundSecondary)
    .clip(RoundedCornerShape(12.dp))
    .border(...)
)
```

Reasons: `NovaSurface` clips correctly out-of-the-box, exposes ripple semantics matching `clickable(enabled=…)`, and serves as the single styling surface across the design system.

---

## Modifiers

### Rule 1: `modifier` is the first parameter

```kotlin
// ✓
@Composable
fun GameScoreCard(modifier: Modifier = Modifier, score: Int) { ... }

// ✗
@Composable
fun GameScoreCard(score: Int, modifier: Modifier = Modifier) { ... }
```

### Rule 2: **never mutate** a passed-in `modifier`

```kotlin
// ✗ Mutating modifier inside the Composable
@Composable
fun MyWidget(modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxWidth().padding(8.dp)) { ... }   // mutated!
}
```

Either:
- Apply everything on the **caller side**: `MyWidget(modifier = Modifier.fillMaxWidth().padding(8.dp))`, or
- Build the **entire** internal modifier without consuming the input:
  ```kotlin
  @Composable
  fun MyWidget(modifier: Modifier = Modifier) {
      val internal = Modifier.fillMaxWidth().padding(8.dp)
      Box(modifier = internal) { ... }
      // — but then why expose modifier at all? Likely shouldn't.
  }
  ```

Passed modifiers must never be modified inside composables.

### Rule 3: when a Composable wouldn't be reused externally, it can **omit `modifier`** entirely

Internal screen-private Composables that won't be placed anywhere else don't need a `modifier` parameter. Reusable ones do.

---

## No early returns inside `@Composable`

Compose's runtime tracks recomposition by slot; early returns break that contract.

```kotlin
// ✗
@Composable
fun Foo(state: State?) {
    if (state == null) return
    ...
}

// ✓
@Composable
fun Foo(state: State?) {
    if (state == null) return@Foo  // — still bad in some lints; prefer:
}

// ✓
@Composable
fun Foo(state: State?) {
    if (state != null) {
        // …
    }
}
```

---

## Spacing

### `NovaTheme.spacings` is for **paddings and margins only**

✗ Don't use `spacingN` for corner radius, stroke width, icon size, or any non-spacing dimension. Use a literal `Dp` or define a feature constant.

```kotlin
// ✓ — padding
modifier = Modifier.padding(NovaTheme.spacings.spacing12)

// ✗ — corner radius
shape = RoundedCornerShape(NovaTheme.spacings.spacing12)

// ✓ — corner radius is its own thing
shape = RoundedCornerShape(12.dp)
```

### Symmetric padding by default

If top padding is `spacing12`, bottom should usually also be `spacing12`. Aids reuse and lets sub-elements be reordered without re-tuning gaps.

```kotlin
modifier = Modifier
    .padding(vertical = NovaTheme.spacings.spacing12)
    .padding(horizontal = NovaTheme.spacings.spacing16)
```

### `VerticalSpacer { spacingN }` / `HorizontalSpacer { spacingN }`

✓ `VerticalSpacer { spacing16 }`
✗ `Spacer(Modifier.height(16.dp))`

---

## Resources and strings

### No hardcoded UI strings

Always extract:
```kotlin
// ✓
Text(text = stringResource(RCommon.string.transfer_send))

// ✗
Text(text = "Send")
```

### Shared resources live in `common`

`common/src/main/res/values/strings.xml`. Import alias is mandatory to avoid clashes with feature-local `R`:

```kotlin
import io.paritytech.polkadotapp.common.R as RCommon

stringResource(RCommon.string.common_yes)
```

(Project memory `feedback_no_defaults` and CLAUDE.md keep this enforced.)

### No hardcoded user-facing strings in ViewModels

ViewModels never carry user-facing text. Errors → sealed types → Compose mapper to `stringResource`. See `results-and-errors.md § Sealed error types`.

---

## Colors

### Always via theme

```kotlin
// ✓
tint = NovaTheme.colors.textAndIconsPrimary
color = NovaTheme.colors.backgroundSecondary

// ✗ — hardcoded
tint = Color(0xFF000000)
color = Color.White
```

Hardcoded colors are not theme-aware across retro, dark, and future palettes.

---

## Icons

- **Shared** icons (24×24 dp): `design/.../components/icon/vectors/`, accessed as `NovaIcons.<Name>` via the Valkyrie plugin import.
- **Feature-specific** icons (any size): `feature/<X>/impl/.../presentation/compose/components/icons/`.
- All icons are `ImageVector`. No raster drawables for icons. No raw library `Icon { }` — always `NovaIcon`.

### Adding an icon

1. Get the SVG, drop in `design/.../components/icon/vectors/`.
2. Use Valkyrie Android Studio plugin to import as `ImageVector` extension on `NovaIcons`.
3. Reference: `NovaIcon(imageVector = NovaIcons.AlertFilled, contentDescription = "…")`.

---

## Drawing — last resort

**Default to `ImageVector` / Valkyrie SVG.** Custom `Canvas` drawing is allowed only when:

1. The shape genuinely cannot be expressed as paths (e.g. waveform with continuously varying amplitude).
2. Per-frame draw is required (audio meters, particle systems, dynamic charts).

When you must use `Canvas`:
- Make the data **allocation-free per frame** — backing float arrays, not `List<DataClass>`.
- Capture stable lambdas; don't take a `FloatArray` by value or copy every particle in a `mapNotNull` on every frame.
- Comment the drawing algorithm — `Canvas` math is non-obvious; explain the geometry.

✗ Re-drawing static graphics that should be SVG (review example of two paths switched via state).

---

## API-level guards

- **Blur modifier** is API 31+. Check before using; provide a fallback for older devices.
- Heavy effects (real-time blur, complex shaders, large bitmaps): consider battery and DPI. Avoid odd numbers for pixel sizes because half-pixel boundaries jump across DPIs.

---

## File splitting

A Compose file becomes unmanageable when:
- The screen + internal + preview exceeds ~400 lines.
- A nested Composable could be reused but is wedged in private at the bottom.
- A single Composable has 8+ parameters.

When you hit this, **split**:

```
foo/compose/
├── FooScreen.kt              ← screen + internal + preview (~200 lines)
└── components/
    ├── FooHeader.kt          ← reusable header
    ├── FooActionsRow.kt
    └── FooEmptyState.kt
```

---

## Recomposition and stability

Keep composables skippable; read scroll state at the lowest scope. Measure with the Compose compiler report (`stable`/`unstable` per param) + Layout Inspector — don't reason statically.

- **Collections** — `ImmutableList`/`Set`/`Map`, never plain `List`, in any UI-state field. Element type must be stable too: `ImmutableList<T>` is unstable if `T` is. One plain `List` field in a sealed subtype taints the parent → the whole screen `StateFlow`. Convert at the boundary (`.toImmutableList()`, `persistentListOf()`).
- **Value equals (transitive)** — Compose-state models are `data class`, **and so are their field types**. A `data class` whose field is an interface backed by a plain `class` (no `equals`) still won't compare equal — `data class` on the variant alone does nothing if a leaf field's impl is a plain class.
- **State at lowest scope** — pass `State` (or `LazyListState`) down and read it in the small consumer, not at a parent that wraps a big subtree. Reading a scroll/animation `State` high up recomposes everything below it on every tick.
- **`derivedStateOf` for derived reads** — wrap a frequently-changing derived read so it notifies only when the result changes. (Scroll: `firstVisibleItemIndex`/`layoutInfo` change every frame; direct reads recompose every frame.)
- **No effect that feeds its own input** — an effect whose callback writes back into the state driving it loops. Common case: a per-item `LaunchedEffect` that mutates the VM-backed list it iterates (also fires on prefetch, not real visibility). Drive it from one `snapshotFlow { … }` instead.

```kotlin
// ✗ val items: List<Item>            ✓ val items: ImmutableList<Item>
// ✗ class FooUiModel(...)            ✓ data class FooUiModel(...)
// ✗ Overlay(info = state.value, isScrolling = list.isScrollInProgress)
// ✓ Overlay(info = state, lazyListState = list)   // read inside the consumer
```

---

## Animations

### Item entry animation in lists

Use the built-in `LazyListScope.animateItem()` (or `animateItemPlacement()` on older Compose). Do not write a custom `MessageEntryAnimation`.

### `reverseLayout = true` for chat

Always `true` in chat feeds. Paging works in one direction only; flipping breaks the paging logic.

---

## Small details (covered in `Rules at a glance`)

- **`clickable(enabled = …)`** — built-in `enabled` param, not `if (enabled) Modifier.clickable {} else this`; the latter breaks ripple transitions.
- **`BackHandler`** — placed inside the screen, not the hosting Fragment. `BackHandler(enabled = state.isModalOpen) { contract.onBackPressed() }`.
- **Imports** — always import; never write `androidx.compose.ui.graphics.Color` inline.
