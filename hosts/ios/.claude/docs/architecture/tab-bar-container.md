# Tab Bar Container

`MainTabBarViewController` is a plain `UIViewController` that owns tab switching itself, replacing `UITabBarController` so the bar could match the Figma spec and support drag-across-tabs. Switching cross-fades outgoing and incoming views over 150 ms (`TabBarContainer.mount`, animated switches only).

The container mounts **either a tab or an SPA browser tab** — `TabBarContentSelection` is `.tab(Int)` or `.spa(UUID)`. An SPA is a peer of the tabs, not a child of one, so it is not pushed or presented; see [SPA Hosting](#spa-hosting).

**`tabBarController` is `nil` everywhere.** Code reaching for it is dead. For the visible screen use `UIWindow.keyWindow?.topmostViewController`, which descends through `TopmostChildProviding`. To reach the container itself use `UIApplication.shared.mainTabBarController`.

## Where Things Live

| Thing | Location |
|---|---|
| Container controller | `polkadot-app/Modules/MainTabBar/MainTabBarViewController.swift` |
| Child mounting | `polkadot-app/Modules/MainTabBar/Container/TabBarContainer.swift` |
| Tab-vs-SPA selection | `polkadot-app/Modules/MainTabBar/Container/TabBarContentSelection.swift` |
| `hidesBottomBarWhenPushed` resolution | `polkadot-app/Modules/MainTabBar/Container/TabBarHiddenPolicy.swift` |
| Re-tap behaviour | `polkadot-app/Modules/MainTabBar/Container/TabBarReselectionPolicy.swift` |
| Scroll-to-top target search | `polkadot-app/Modules/MainTabBar/Container/TabBarScrollToTopLocator.swift` |
| Fold state machine | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldController.swift` |
| Fold state resolution | `polkadot-app/Modules/MainTabBar/Chrome/TabBarVisibilityPolicy.swift` |
| Fold animator utilities | `polkadot-app/Modules/MainTabBar/Chrome/UIViewPropertyAnimator+Cancel.swift` |
| Bar, widgets and safe-area insets | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBottomChromeController.swift` |
| Chrome apply input | `polkadot-app/Modules/MainTabBar/Chrome/TabBarChromeContext.swift` |
| Panel kinds | `polkadot-app/Modules/MainTabBar/Chrome/TabBarPanelKind.swift` |
| Bar view and its parts | `Packages/PolkadotUI/Sources/Components/DSTabBar/` |
| Row geometry (`DSTabBarRow`) | `Packages/PolkadotUI/Sources/Components/DSTabBar/DSTabBarGeometry.swift` |
| Action slots and state | `polkadot-app/Modules/MainTabBar/TabBarSlot.swift`, `Chrome/TabBarSlotMap.swift` |
| Backdrop dimming | `Packages/PolkadotUI/Sources/Components/DSTabBar/DSTabBarBackdropView.swift` |
| Tabs panel | `.../DSTabBar/DSTabBarTabsPanelView.swift`, `DSTabBarPanelLayout.swift`, `DSTabBarChipView.swift` |
| Content panel | `.../DSTabBar/DSTabBarContentPanelView.swift` |
| Top status strip view | `Packages/PolkadotUI/Sources/Modules/MainTabBar/ChainConnectionStatusBarView.swift` |
| Chrome glass surface | `Packages/PolkadotUI/Sources/Components/DSGlassBackground/` |
| SPA tab store and controller pool | `polkadot-app/Modules/Browser/` |
| SPA chip view models | `polkadot-app/Modules/MainTabBar/SPATabChipViewModel.swift` |
| Placeholder trailing content | `polkadot-app/Modules/MainTabBar/TabBarPanelPlaceholderContent.swift` |
| `topmostViewController` | `Packages/UIKitExt/Sources/UIWindow/UIWindow+keyWindow.swift` |
| `mainTabBarController` | `polkadot-app/Common/Extension/UIApplication+MainContainer.swift` |

## Fold State

`.shown` is the full capsule; `.folded` is a leading-edge sliver, tappable to restore. `TabBarVisibilityPolicy.state(isTabRoot:derived:override:)` resolves in order **under `FEATURE_PRODUCTS` (DevCI, Nightly)**:

1. Tab root → `.shown`. **A tab root can never fold.**
2. A recorded per-screen user override (`.shown` / `.folded` / `.none`) wins.
3. Otherwise the `derived` state (from stack content analysis).

Overrides live in `screenOverrides`, weakly keyed on `navigationScreen`, so folding one pushed screen does not leak to another.

**Under `!FEATURE_PRODUCTS` (Release)**, steps 2 and 3 never run — every non-root screen resolves to `.hidden` regardless of `override` or `foldDerived`.

Nothing hides the bar programmatically — presented view controllers cover it structurally. Both LocalAuth screens are transparent by design, so the bar stays *visible* behind re-auth but untappable, since the presentation owns the touch.

## Bar Visibility

**Under `FEATURE_PRODUCTS`, the bar folds when *any* controller at or below the target sets `hidesBottomBarWhenPushed`, not just the top one.** `UITabBarController` behaved this way and screens rely on it. `TabBarHiddenPolicy.deriveFoldState(in:showing:)` scans `stack[...targetIndex].dropFirst()`; the root is excluded, so a tab root setting the flag does not fold.

**Under `!FEATURE_PRODUCTS`, every non-root screen is `.hidden` regardless of the `hidesBottomBarWhenPushed` flag**, so this scan has no effect there.

`stackAfterCancelledPop(stack:staying:)` covers the window where UIKit has already mutated `viewControllers` but the transition is reversing — a cancelled pop restores the *staying* screen's state, not the target's.

## Safe Area

Two insets stack at the bottom and go on **different** controllers so they compose instead of overwriting. They are unchanged by the top status strip and still go on the two child controllers below; **the top inset is the only inset the container writes on itself** — a constant, set once (see [Top Status Strip](#top-status-strip)).

| Inset | Applied to |
|---|---|
| Bar clearance | the tab's `UINavigationController` — or the tab controller itself when there is none (Scan) |
| Widget height | that nav's `topViewController` — summed onto the tab controller when there is no nav |

Split because they change at different rates — the nav controller is stable for the tab's lifetime, widget height follows the top screen. `updateContentSafeAreaInset()` zeroes the previously-adjusted controller before writing the new one.

`occupiedHeight` is `DSTabBarView.preferredHeight()` on a tab root, the raw bottom safe-area inset elsewhere; `contentClearance` is `occupiedHeight` minus that inset, floored at 0. Neither reads fold state — both depend only on `isTabRoot`.

**Under `FEATURE_PRODUCTS`, clearance is contributed only on a tab root**, which is narrower than the bar being `.shown`. Off the root a screen can show the full capsule — no override, no fold requested — while reserving zero space, so the bar floats over its bottom content and swallows touches in the capsule rect. Pushed screens should either set `hidesBottomBarWhenPushed` or keep interactive content clear of the bottom.

**Under `!FEATURE_PRODUCTS`, non-root screens are always `.hidden`, so clearance ⟺ tab root ⟺ `.shown`** — the bar cannot float over pushed screen content or swallow touches, and the advice to pushed screens does not apply. `contributesClearance(isTabRoot:)` itself is unchanged and correct in both arms.

### `apply` vs `applyLayout`

- `apply(_:animatingAlongside:)` sets the fold inputs (`isTabRoot`, `stackFolds`, `navigationScreen`), retargets, then `refresh()` resolves and animates the fold.
- `applyLayout(_:animatingAlongside:)` retargets only, leaving fold inputs untouched.

**Mid-gesture, use `applyLayout`.** An interactive pop drives the fold through `setFoldProgress`; a full `apply` would let `refresh()` snap the bar and fight the gesture. `track(of:showing:animated:)` calls `applyLayout` while interactive, `apply` once `notifyWhenInteractionChanges` settles.

`viewSafeAreaInsetsDidChange()` and `TabBarContainer`'s `onContentInsetInvalidated` also use `applyLayout`, re-deriving from `container.selectedController` instead of reusing the chrome's last-applied targets — reusing them let a late `didShow` from a background tab pin insets to a controller no longer visible. Fold inputs are still set only by `apply`, unchecked against the selected tab; no known live path, since `ModuleNavigator` selects the tab before navigating.

`updateLayout` no-ops while the chrome's view has no window — a `.fullScreen` presentation elsewhere detaches it, where `safeAreaInsets` reads zero and clearance would compute too large. Last-applied insets hold until reattachment fires `viewSafeAreaInsetsDidChange`.

## Top Status Strip

A permanent 20pt strip at the top of `MainTabBarViewController` holds one `ChainStatusRingView` per chain for the same three `ChainConnectionTarget`s (chat, bulletin, assethub). There is no visible chain name. **The strip is informational only**: no tap handling, no fold, no hide, constant height, and it ships in both `FEATURE_PRODUCTS` arms.

The chain name and state survive only as the ring's accessibility label, which the ring owns.

`installStatusBar()` writes `additionalSafeAreaInsets.top = ChainConnectionStatusBarView.preferredHeight` on **the container itself**, once, in `viewDidLoad`. It is a constant and is never recomputed in `viewSafeAreaInsetsDidChange`. UIKit propagates the combined inset (system top + 20) down through each mounted nav controller to every screen it pushes, so **a pushed screen inherits the clearance with no bookkeeping**.

**There is one view, not two**: the `UIHostingController`'s, added straight to the container view with leading, trailing and **bottom pinned to `view.safeAreaLayoutGuide.snp.top`** — that guide already includes the 20pt once the inset is set, so the host occupies exactly the band the inset reserved. **No height constraint**; height comes from the SwiftUI view's own `.frame(height: Self.preferredHeight)` as intrinsic content size. The host's `backgroundColor` is `.clear` — **the strip paints no fill of its own**. What shows behind the system status bar and behind the icons is the container's own `.bgSurfaceMain`.

**Install order in `viewDidLoad` is load-bearing.** `installStatusBar()` runs *before* `installChromeController()` so the bottom chrome stays topmost, and mounted tab children insert at subview index 0 so they stay behind the strip. `TabBarContainer` is unchanged — children still mount full-bleed into the container's own view.

## Re-tap

`TabBarReselectionPolicy.action(for:)`, in order: modal presented on the target → `.ignore`; stack deeper than its root → `.popToRoot`; otherwise `.scrollToTop`.

Pop-to-root matches `UITabBarController`. Scroll-to-top is the addition and fires only at the tab root, so returning from a detail screen preserves the root's scroll position. `TabBarScrollToTopLocator` picks the scroll view heuristically — visible area, vertical scrollability, depth. A screen cannot opt in or nominate one.

## Selection Gesture

`DSTabSelectionRecognizer` tracks a single touch beginning anywhere `DSTabBarView.hitTest` claims — the capsule only. The folded bar's tap target lives on `TabBarChromePassthroughView`, which is full-bleed and can receive touches at the screen edge that the inset container cannot; a tap there routes through `setUserOverride(.shown,)`, the same path `onFoldChangeRequested` uses. The recognizer cancels once vertical travel exceeds `DSTabBarMetrics.selectionCancelVerticalSlop`; horizontal travel never cancels, since it drives drag-across-tabs (clamped by `DSTabBarGeometry.clampedPillOriginX`).

Touch-to-item resolution is `resolvedTarget(atX:) -> Target?` (`.tab(Int)` or `.action(Int)`), tested in order: action frames, then nearest tab. A press beginning on an action creates no `dragState`, so the lens never lifts or parks on one. Only `.tab` taps can drag or perform selection; `.action` fires `onActionTapped` in the `.ended / .select` branch, guarded by `!isFolded` to match the `.began` phase.

Cancelling does *not* hand the touch to the scroll view underneath — UIKit hit-tests once at `touchesBegan`, not on every move.

**Two accepted gaps**, reviewed and kept deliberately — neither is an undiscovered bug:

- A short, fast vertical flick stays under the slop and `DSTabBarFoldDecision.decideFromShown` tests `velocityX`, so it falls through to `.select`. On the selected tab of a deep stack that reaches `popToRoot`.
- A wide drag-across-tabs whose arc deviates past the slop cancels itself mid-drag.

They are coupled — raising the slop widens the second, lowering it widens the first. Decoupling needs an axis-relative test: cancel only when vertical travel both exceeds the slop and dominates horizontal.

## Glass Container

`DSGlassContainerView` owns exactly one `DSGlassBackgroundView` surface; the chrome content (bar, panel, widgets) lives inside that surface's `contentView`. No `UIGlassContainerEffect` or merge/spacing tuning.

The container is constrained to the capsule's geometry: centred, `width <= 500`, `width == superview - 21*2` at `.high`. `DSTabBarView` lays out against its own bounds and no longer insets via `DSTabBarGeometry.capsuleFrame`.

One `.capsule` shape serves both states: a true capsule at 62pt tall when collapsed, clamped to the same radius when the panel expands the container. No state-dependent shape switching.

*Rationale:* Apple's guidance (WWDC 2025 session 284, "Build a UIKit app with the new design") is that glass elements should not overlap — one floating layer, not several merged shapes. Pre-iOS 26, the same surface renders blur + tinted substrate + border + shadow as a single material; the old `setCapsuleGlassHidden` workaround is gone.

## Tabs Panel and Content Panel

`DSTabBarTabsPanelView` holds open-SPA chips in a scroll view with no background of its own; installed by `TabBarBottomChromeController` and pinned `bottom == barView.top` with `leading/trailing.equalToSuperview()` to fill the container horizontally. Chips are a fixed 5-column grid (`DSTabBarPanelLayout`); height is `min(contentHeight + capsuleHeight, availableHeight)` since the container stacks the panel and the capsule.

- Long-press a chip → context menu with Close (`onChipCloseRequested`).
- Tap a chip → mount that SPA and close the panel.
- `.spaTabs` panel closes on: outside tap, tab selection, `select(tab:)`, fold to `.folded`, chip list emptying, or no available height.

The outside tap is why `TabBarChromePassthroughView` is no longer purely passthrough: while any panel is open it claims self-hits (`isOutsideTapEnabled`) so the recognizer can fire. The recognizer's delegate accepts only touches landing on the chrome view itself, so bar and widget touches are unaffected. The passthrough view also carries the fold grab zone and checks it *before* the outside-tap flag, so a folded bar unfolds rather than closing a panel.

Chips reuse views across updates — `setChips` rebuilds only when the id sequence changes, otherwise it re-applies in place, and `DSTabBarChipView.apply` skips icon reload when the id is unchanged. The "chips empty or no available height" auto-close is guarded on `openPanel == .spaTabs` so it does not close an unrelated content panel on SPA mount/unmount.

### Content Panel

`DSTabBarContentPanelView` hosts any `HashableContentConfiguration` via `makeContentView()` — the same seam `AppWidgetContentViewController` uses — and reuses the content view when `defaultReuseIdentifier` is unchanged. It knows nothing about the content. Content must be self-sizing: height is `systemLayoutSizeFitting` clamped by `DSTabBarPanelLayout.panelHeight(contentHeight:availableHeight:)` (same layout the chips use), and it returns `capsuleHeight` when it has no configuration or no width yet.

Panel state is `TabBarBottomChromeController.openPanel: TabBarPanelKind?` (`.spaTabs` / `.content(TabBarAction)`), so an open content panel names the action that owns it. Both panels' `setPanel` run through one animator, so closing one panel is automatic when opening another — exclusivity is structural.

## SPA Hosting

`MainTabBarViewController` conforms to `SPAHosting` (`openProduct(page:)`, `minimizeSPA()`, `closeSPA(tabId:)`). Every entry point routes there through `UIApplication.shared.mainTabBarController` — `ModuleNavigator.openProduct`, `ProductsNavigationRouter.navigateTo`, and `SPAWireframe`. **SPAs are no longer pushed onto the Browse stack or presented full-screen**; the old push/present/dedupe logic in `ModuleNavigator` and `SPAWireframe.showProductSPA` is gone.

Flow of one open:

1. `SPABrowserCoordinator.findOrCreateTab(for:)` matches on `dotDomain`; an existing tab with a different `page` is navigated in place rather than duplicated.
2. `SPAControllerPool` vends (and caches) one `SPAViewController` per tab id, all sharing a single lazily created `SPAFlowState`.
3. `container.mountSPA(_:for:)` sets `selection = .spa(id)` and cross-fades it in.
4. `chromeController.apply(.spa(controller))` — `TabBarChromeContext.spa` marks it `isTabRoot: false, foldDerived: .folded`. **Under `FEATURE_PRODUCTS`, an SPA behaves like a pushed screen: the bar folds and contributes no clearance.** Under `!FEATURE_PRODUCTS` this resolves to `.hidden` rather than folded; the path is unreachable there anyway, since `openProduct` routes to `presentProduct` (a modal `.pageSheet`) instead of `mountSPA`, as the `#if !FEATURE_PRODUCTS` block in `MainTabBarViewController` describes.

`TabBarContainer.select(index:)` re-mounts even when the index is unchanged if the current selection is an SPA, and reselection of the already-selected tab index is treated as a real switch in that case (`handleSelection`). `minimizeSPA()` returns to `currentIndex`; `closeSPA` drops the pooled controller and the stored tab, then minimizes only if that tab was the mounted one.

Chip state flows the VIPER way: `MainTabBarInteractor` observes `SPATabManaging` (`sendOnSubscription: true`), the presenter maps tabs through `SPATabChipViewModelFactory` (name from `ProductHost`, icon from the `.dot` domain via `DotNsResolver`), and the view converts to `DSTabBarChip`. `applyChips()` re-sends on every mount/unmount so `selected` tracks `mountedSPATabId`.

## Adding a Tab

Add a case to `TabBarItem` (`polkadot-app/Modules/MainTabBar/MainTabBarProtocols.swift`) with a localized `title` and an asset, build the controller in `TabFactory.view(for:)`, and add an `AccessibilityID.Tab` entry. `DSTabBarView` lays items out from `DSTabBarRow`; nothing is hardcoded per tab count.

**Tab count affects all unit widths.** `unitWidth` scales with `unitCount`, so adding a tab narrows every item. All widths derive from `DSTabBarRow.unitWidth` divided across `itemCount`; no per-tab hardcoding means the layout adapts automatically.
