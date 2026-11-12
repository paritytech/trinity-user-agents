# Nova Widget Inventory

> **Loaded on demand only.** `ui-compose.md` cites the most-used Nova widgets inline. Pull this doc when you need to discover whether a Nova wrapper exists for what you're about to write.

Full catalog of `Nova*` composables and Compose helpers in `design/src/main/.../components/`. **Always check this list before introducing a new widget.**

> Rules around Nova widget usage live in `code/ui-compose.md § Nova wrappers`. This doc is the **catalog** only — pull it when you need to discover whether a Nova wrapper exists for what you're about to build.

## Catalog

| Need | Nova widget |
|---|---|
| Button (primary, secondary, destructive, accent, warning, transparent) | `NovaButton` |
| Text-only button | `NovaTextButton` |
| Text — `String` | `NovaText` |
| Text — `AnnotatedString` | `NovaText` (overload) |
| Text field | `NovaTextField` |
| Icon | `NovaIcon` |
| Container with background / border / shape / elevation / ripple | `NovaSurface` |
| Modal sheet | `NovaModalBottomSheet` |
| Alert dialog | `NovaAlertDialog` |
| Drag handle | `NovaBottomSheetDragHandler` |
| Async image | `NovaAsyncImage` |
| BlurHash placeholder | `BlurHashPlaceholder` |
| QR code | `QrCode` (no `zxing` — `qrose`) |
| Loading shimmer | `Shimmer` |
| Loading screen | `LoadingScreenState` |
| Empty state | `EmptyScreenState(title, message)` |
| Error state | `DefaultErrorState(text)` |
| Circular progress | `NovaCircularProgressIndicator` |
| Linear progress | `NovaLinearProgressIndicator` |
| Arc / segmented progress | `SegmentedArcIndicator` |
| Tooltip | `NovaTooltip` |
| Top bar (centered) | `NovaCenteredTopBar` |
| Top bar (compact) | `NovaCompactTopBar` |
| Top bar (expanded) | `NovaExpandedTopBar` |
| Address avatar | `NovaAddressAvatar` |
| User avatar | `NovaUserAvatar` |
| Checkbox | `NovaCheckBox` |
| Tri-state checkbox | `NovaTriStateCheckbox` |
| Switch | `NovaSwitch` |
| Dropdown menu | `NovaDropdownMenu` |
| Dropdown header | `NovaDropdownHeader` |
| Dropdown item | `NovaDropdownItem` |
| Menu option | `NovaMenuOption` |
| Vertical gap | `VerticalSpacer { spacingN }` |
| Horizontal gap | `HorizontalSpacer { spacingN }` |
| Mnemonic display (protected) | `ProtectedMnemonic` |
| Mnemonic grid | `MnemonicHolder` |

## Theme tokens

- `NovaTheme.colors.*` — background, textAndIcons*, applied*, fill*, fillDark*, semantic (success/error/warning).
- `NovaTheme.spacings.spacingN` — 1, 2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48 — and friends.
- `NovaTheme.typography.*` — 14 named text styles (titleXXL, headline, bodyM, caption1, etc.).

(Usage rules — paddings-only for spacings, no raw `Color(...)` — live in `code/ui-compose.md`.)

## When the catalog doesn't cover your need

1. Confirm the design *actually* needs something new (not a tweak of an existing variant).
2. If reusable across features: add to `design/` — interface in `components/`, theming via `NovaTheme.colors`.
3. If feature-private: place under `feature/<X>/impl/.../presentation/compose/components/`.
4. Use `NovaSurface` as the container primitive in the new widget; don't reinvent its clipping/border/elevation behavior.
