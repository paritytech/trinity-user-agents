# Tab Bar Container

The bottom tab bar is SwiftUI inside a thin UIKit shell. SwiftUI draws the bar row, the center control, the center panel and the shelf. UIKit keeps the three jobs that need a `UIViewControllerTransitionCoordinator` or a `UIView` hit test: the fold, the passthrough hit testing, and the resting bottom clearance that content is inset by. The reasoning is in [ADR 0001](../../../docs/adr/0001-uikit-shell-around-swiftui-tab-bar.md).

`MainTabBarViewController` is a plain `UIViewController`. It mounts one child at a time, owns the top status strip, and writes the bottom insets on the tab and content controllers.

## Model in brief

- The bar row has four fixed slots around a center hole. Slots select tabs or open the shelf.
- The center control sits in the hole and morphs into the center panel. Every size is read from one progress value between 0 and 1.
- One value, `TabBarPanelKind?`, says which panel is open: `.shelf` or `.content(.scan)`. The shelf and the center panel cannot both be open.
- One value, `TabBarVisibilityState` (`shown`, `folded`, `hidden`), says where the bar sits. The fold controller animates it.

## Ownership

```
MainTabBarViewController            container, status strip, widget and inset layout, SPAHosting
├── TabBarContainer                 mounts one tab or one SPA
├── ChainConnectionStatusBarView    20 pt strip, informational only
└── TabBarBottomChromeController    bar, panels, fold, outside tap, bar items
    └── TabBarChromePassthroughView full-bleed, hit tests only the grab zone and the outside-tap area
        └── TabBarChromeSurfaceView fold transform target, bar layout
            └── TabBarBarHostView   accepts touches in the bar row and the open panel's rise
                └── UIHostingController<TabBarBarView>
                                    SwiftUI bar row, center control, shelf
```

## Where Things Live

| Thing | Location |
|---|---|
| Container controller | `polkadot-app/Modules/MainTabBar/MainTabBarViewController.swift` |
| Widget and inset layout | `polkadot-app/Modules/MainTabBar/MainTabBarViewController+Widgets.swift` |
| SPA hosting | `polkadot-app/Modules/MainTabBar/MainTabBarViewController+SPA.swift` |
| Child mounting | `polkadot-app/Modules/MainTabBar/Container/TabBarContainer.swift` |
| Tab-vs-SPA selection | `polkadot-app/Modules/MainTabBar/Container/TabBarContentSelection.swift` |
| `hidesBottomBarWhenPushed` resolution | `polkadot-app/Modules/MainTabBar/Container/TabBarHiddenPolicy.swift` |
| Re-tap behaviour | `polkadot-app/Modules/MainTabBar/Container/TabBarReselectionPolicy.swift` |
| Scroll-to-top target search | `polkadot-app/Modules/MainTabBar/Container/TabBarScrollToTopLocator.swift` |
| Bottom chrome: bar, panels, fold, clearance | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBottomChromeController.swift` |
| Chrome apply input | `polkadot-app/Modules/MainTabBar/Chrome/TabBarChromeContext.swift` |
| Passthrough hit testing | `polkadot-app/Modules/MainTabBar/Chrome/TabBarChromePassthroughView.swift` |
| Fold transform target, bar width | `polkadot-app/Modules/MainTabBar/Chrome/TabBarChromeSurfaceView.swift` |
| Fold state machine | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldController.swift` |
| Fold state resolution | `polkadot-app/Modules/MainTabBar/Chrome/TabBarVisibilityPolicy.swift` |
| Fold drag decision | `polkadot-app/Modules/MainTabBar/Chrome/DSTabBarFoldDecision.swift` |
| Panel state machine | `polkadot-app/Modules/MainTabBar/Chrome/TabBarPanelController.swift` |
| Panel kinds | `polkadot-app/Modules/MainTabBar/Chrome/TabBarPanelKind.swift` |
| Bar row view (SwiftUI) | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBarView.swift` |
| Bar row state | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBarViewModel.swift` |
| Shelf content (SwiftUI) | `polkadot-app/Modules/MainTabBar/Chrome/TabBarShelfView.swift` |
| Slots | `polkadot-app/Modules/MainTabBar/TabBarSlot.swift`, slot list in `MainTabBarPresenter.swift` |
| Slot-to-tab index mapping | `polkadot-app/Modules/MainTabBar/Chrome/TabBarSlotMap.swift` |
| Tab items | `TabBarItem` in `polkadot-app/Modules/MainTabBar/MainTabBarProtocols.swift` |
| Center panel content height | `polkadot-app/Modules/MainTabBar/ScanPanelContentHeight.swift` |
| Center panel content (scanner) | `polkadot-app/Modules/MainTabBar/ScanPanelViewController.swift` |
| Onboarding tips | `polkadot-app/Modules/MainTabBar/Chrome/TabBarTipController.swift`, `polkadot-app/Modules/MainTabBar/Tips/` |
| Floating widgets | `polkadot-app/Modules/MainTabBar/TabBarWidgetController.swift`, `MainTabBarFloatingWidgetStackView.swift` |
| Bar, center control, shelf container | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarContainer.swift`, `DSTabBarCenterControl.swift` |
| Bar outline, morph shape, geometry | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarShape.swift`, `DSTabBarMorphShape.swift`, `DSTabBarStyle.swift` |
| Bar material | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarMaterial.swift`, `DSTabBarVariableBlur.swift` |
| Bar item and chip models | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarItem.swift`, `DSTabBarChip.swift` |
| Bar icons | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarConnectionGlyph.swift`, `DSTabBarTabsGlyphView.swift` |
| Top status strip | `Packages/PolkadotUI/Sources/Modules/MainTabBar/ChainConnectionStatusBarView.swift` |
| SPA tab store and controller pool | `polkadot-app/Modules/Browser/SPABrowserCoordinator.swift`, `polkadot-app/Modules/Browser/Pool/SPAControllerPool.swift` |
| SPA chip view models | `polkadot-app/Modules/MainTabBar/SPATabChipViewModel.swift` |
| `topmostViewController` | `Packages/UIKitExt/Sources/UIWindow/UIWindow+keyWindow.swift` |
| `mainTabBarController` | `polkadot-app/Common/Extension/UIApplication+MainContainer.swift` |

## Bar Row

`DSTabBarContainer` lays out two slots left of the center hole and two right of it. The bar row is 62 pt tall (`DSTabBarStyle.height`) with a 21 pt bottom gap. The bar row takes four `TabBarSlot` values, in the order the presenter lists them: Chat, Pocket (`wallet`), Browse, and Apps (`.action(.spaTabs)`).

`DSTabBarStyle.rowWidth(inAvailableWidth:)` gives the bar row width. It is the available width minus 21 pt on each side, capped at 500 pt. The fold controller and the center panel sizing both use it.

**Center hole.** The center button is 64 pt (`centerButtonSize`). The hole radius is derived as `notchRadius = centerButtonSize / 2 + notchRingGap`, which gives 35 pt with the 3 pt `notchRingGap`. The hole center sits 22 pt below the top of the bar row (`notchCenterY`), so the center button stands 10 pt above the bar. Where the hole meets the bar's top edge, both corners are rounded with `notchFilletRadius` (4 pt). The rounding shrinks as the hole reaches higher above that edge, so it fades out smoothly when the shelf rises past the hole. `DSTabBarShape` and `DSTabBarShadowMask` subtract the same cut-out, the hole plus its rounded corners, from the bar. The rim strokes the bar outline and the edge of the hole.

**Selection pill.** `DSTabBarContainer` draws the pill behind the selected slot, from its `selectedSlot` parameter. It is a capsule filled with `bgSurfaceContainerInverted` at 12% opacity, inset 4 pt vertically. It slides between slots on a spring (response 0.4, damping 0.8) and snaps under Reduce Motion. The Apps slot never takes the pill. It reads as active through its color.

- A tab slot selects a tab. Taps are plain taps. There is no drag across tabs.
- The Apps slot opens the shelf. It shows the open-product count and reads as active while the shelf is open or a product is mounted.
- `TabBarSlotMap` converts between bar position (item index) and tab position (tab index). Only tab indices cross into `TabBarContainer`.
- Titles appear under the icons only when the interactor enables labels (`setLabels(visible:)`).
- While the bar is folded, a slot tap only unfolds the bar. It does not select.

## Center Control and Center Panel

The center control, `DSTabBarCenterControl`, sits in the hole. A tap or an upward drag opens it. A downward drag or a tap closes it. A flick decides on its own, and otherwise the halfway point decides. The control uses the same material as the bar (`DSTabBarMaterial`), with its own rim and shadow.

Every width, height and radius is read from one progress value, so the shape is valid at any point of a drag. The content (the scanner) fades in from progress 0.45. The chrome builds the content the first time the control shows it, and the content is released when the control removes it.

The panel declares its height rather than measuring the content. `ScanPanelContentHeight.height(forPanelWidth:)` computes it from the panel width. The chrome takes that width from `DSTabBarStyle.rowWidth(inAvailableWidth:)` of its own view width, not from the hosting view, because the hosting view is laid out after the chrome and is 0 wide on the first pass. Measuring a live camera would start a second capture session. The chrome recomputes the height on layout and whenever the content reports a change.

**Search focus.** The scan panel's search field is always present. Focusing it reports through `ScanPanelViewController.onSearchFocusChanged`. `MainTabBarViewController` passes that to `TabBarBottomChromeController.setBarHidden(_:)`. `DSTabBarContainer`'s `isBarHidden` fades the bar row out (0.2 s ease) while the panel stays. The bar returns when focus ends. Closing or folding the panel also ends focus.

While search is focused, the panel declares `.fill` instead of a height. The chrome caps the panel with `DSTabBarStyle.centerContentHeight(fittingRise:)` at `availablePanelHeight`, the room between the bar row and the bottom of the chain status strip. A filled panel's top meets the strip.

**Keyboard.** While the scan panel is open, `TabBarBottomChromeController` lifts the chrome until the keyboard covers the center button's foot by `keyboardButtonOverlap` (4 pt). Any other keyboard, or a closed panel, leaves the chrome at rest. The lift shrinks `availablePanelHeight` by the same amount.

The center panel can also open from code. `ModuleNavigator.openScanPanel()` sets `.content(.scan)`, and the control follows through `setCenterPanelOpen`. Closing the panel from any source closes the control.

Under Reduce Motion the geometry snaps, and only the content cross-fades.

## Shelf

The Apps slot opens the shelf, which grows upward from the bar row. Its height is the content height, capped at the space between the bar row and the top safe area. Beyond the cap the grid scrolls.

- The top row holds a connection status icon and a settings icon. Neither is a tab. Connection status opens as a page sheet with medium and large detents. Settings opens as a modal navigation stack. Both close the shelf first.
- The grid holds one cell per open product. Tapping a cell mounts that product and closes the shelf. A long press reveals a close control. The cell's Close accessibility action does the same.

## Panels

`TabBarPanelKind` is `.shelf` or `.content(TabBarAction)`. Because the shelf and the center panel are one value, opening one closes the other.

`TabBarPanelController` runs the transition. Tapping a different action closes the open panel, then opens the new one when the close completes. Tapping the open panel's own action closes it.

The open panel closes on:

- a tap outside the open panel (`isOutsideTapEnabled` on the passthrough view),
- a tab selection,
- a connection status or settings tap,
- a product tap,
- any fold state other than `shown`.

## Hit Testing and Bar Rise

UIKit stops hit testing at a view's bounds, so the bar's hosting view grows to contain an open panel. Its height is the bar row plus a rise: `DSTabBarStyle.centerPanelRise` for the center panel, or the shelf's available height for the shelf. The row stays at the bottom. The rise is set when the panel state commits and is not animated. The SwiftUI springs animate what is drawn inside it.

`TabBarChromePassthroughView` claims a touch in only two cases: the folded grab zone, and the dismiss area while a panel is open. Every other touch goes to the content behind the chrome. `TabBarChromeSurfaceView` returns nil for hits on its own empty area. `TabBarBarHostView` accepts touches in its live region, which is the bar row plus the open panel's rise.

The fold pan recognizer starts only on the bar row outside the center hole, and only for a drag that leads horizontally.

## Fold

The three states are `shown`, `folded` and `hidden`.

- **Shown.** The bar sits at the bottom of the screen.
- **Folded.** The bar slides to the leading edge and leaves a 19 pt sliver. A 44 pt grab zone on the passthrough view restores it on tap.
- **Hidden.** The bar slides fully off the leading edge. There is no sliver and no grab zone, so tab navigation is not available on that screen. The only conformer to `TabBarChromeVisibilityCustomizing` in the app is `ChatViewController`.

`TabBarVisibilityPolicy.state(isTabRoot:derived:override:)` resolves the state in this order:

1. A tab root is always `shown`. A tab root never folds.
2. A per-screen user override (`folded` or `shown`) wins.
3. Otherwise the derived state applies.

The derived state comes from `TabBarHiddenPolicy.deriveFoldState(in:showing:)`. It is `hidden` when the target opts out. It is `folded` when any controller above the root, up to the target, sets `hidesBottomBarWhenPushed`. The root is excluded, so a tab root that sets the flag does not fold.

Overrides are stored per screen, weakly keyed on the navigation screen, so folding one pushed screen does not affect another. The fold controller ignores override requests while the bar is `hidden`.

**Interactive pops.** The transition coordinator drives the fold. `setInteractiveTarget` runs inside `animate(alongsideTransition:)`. When the interaction ends, the chrome commits the incoming screen's state. If the pop was cancelled, it commits the staying screen's state, rebuilt with `TabBarHiddenPolicy.stackAfterCancelledPop`.

**Animation.** A spring (duration 0.45, damping 0.85) takes the gesture's velocity as its initial velocity.

`DSTabBarFoldDecision` classifies a horizontal drag on the bar row. From `shown`, a fast leftward flick folds, and any other drag selects. From `folded`, a tap or a fast rightward flick unfolds. Otherwise the bar unfolds if the drag passes 30% of the fold distance, and settles back to folded if it does not.

## Clearance and Safe Area

Two bottom insets stack, and they go on different controllers so they compose.

| Inset | Applied to |
|---|---|
| Bar clearance | the tab's `UINavigationController`, or the tab controller itself when there is none |
| Widget height | the top screen's controller, or added to the tab controller when there is no navigation controller |

The two change at different rates. The navigation controller is stable for the tab's lifetime, and the widget height follows the top screen. `updateContentSafeAreaInset()` zeroes the controller it inset last before it writes the new one.

- `restingBarHeight` is 83 pt (`DSTabBarStyle.restingHeight`) on a tab root. Elsewhere it is the raw bottom safe area.
- `restingBottomClearance` is `restingBarHeight` minus the bottom safe area, floored at 0.
- Clearance is contributed only on a tab root. A pushed screen that does not set `hidesBottomBarWhenPushed` reserves no space, so the bar floats over its bottom content and takes touches in its capsule. Such a screen should either set the flag or keep interactive content clear of the bottom.
- The shelf and the center panel overlay content. They do not change clearance.

**Recalculation.** Three events recompute the insets from `container.selectedController`: the container's safe area changes, the chrome's safe area changes, and a tab mounts. Re-deriving from the selected controller stops a late `didShow` from a background tab pinning insets to a screen that is not on screen.

**Fold inputs.** `isTabRoot`, the derived state and the screen change only through `applyChrome(_:animatingAlongside:)`. The inset path reads them but never writes them.

**No window.** `updateWidgetLayout` returns while the view has no window, so a `.fullScreen` presentation elsewhere does not produce bad insets. The last applied insets hold until the view reattaches.

## Top Status Strip

A 20 pt strip sits at the top of the container. It shows one ring per connection row. Each ring's accessibility label is the chain title and its state. No chain name is drawn. The strip has no tap handling, does not fold, and does not hide.

- `installStatusBar()` sets `additionalSafeAreaInsets.top` to 20 once, in `viewDidLoad`. Every mounted navigation controller passes that inset to the screens it pushes, so a pushed screen's top safe area already includes the strip.
- The strip paints no fill. The container's `.bgSurfaceMain` background shows behind the system status bar and the strip.
- `installStatusBar()` runs before `installChromeController()`, so the bar stays above the strip. Mounted children insert at index 0, so they sit below it.

## Mounting and Re-tap

`TabBarContainer` mounts one child at a time, full bleed into the container's view. A tab switch cross-fades over 0.15 s. The incoming view starts at scale 0.998. The outgoing view is removed when the fade completes. Non-animated mounts skip the fade.

A tap on the selected tab is a re-tap. `TabBarReselectionPolicy.action(for:)` checks these cases in order:

1. A modal is presented from the target: ignore.
2. The navigation stack is deeper than its root: pop to root.
3. Otherwise: scroll to top.

Scroll-to-top fires only at the tab root, so returning from a detail screen keeps the root's scroll position. `TabBarScrollToTopLocator` picks the scroll view by visible area, then vertical scrolling, then depth. A screen cannot nominate one.

While an SPA is mounted, a tap on the selected tab switches back to the tab. It is not treated as a re-tap.

## SPA Hosting

`TabBarContentSelection` is `.tab(Int)` or `.spa(UUID)`. An SPA is a peer of the tabs. It is not pushed onto a stack or presented over the container. `MainTabBarViewController` conforms to `SPAHosting`. `ModuleNavigator`, `ProductsNavigationRouter`, `SPAWireframe` and `ProductOpener` reach it through `UIApplication.shared.mainTabBarController`.

Opening a product:

1. `SPABrowserCoordinator.findOrCreateTab(for:)` matches on `dotDomain`. An existing tab with another page is navigated in place.
2. `SPAControllerPool` vends one `SPAViewController` per tab id.
3. `container.mountSPA(_:for:)` sets the selection to `.spa(id)` and cross-fades the controller in.
4. `applyChrome(.spa(controller))` sets `isTabRoot: false` and a derived state of `folded`. The bar folds and contributes no clearance.

`presentProduct(page:)` is the one-shot path for funding products. It presents a page sheet at 92% of the maximum detent. The product never enters the tab store, so closing it leaves no chip.

**Chips.** `MainTabBarInteractor` sends the open tabs to the presenter (`didReceiveSPATabs`). The presenter maps them through `SPATabChipViewModelFactory`, and the view passes them to the chrome as `DSTabBarChip`. `applyChips()` runs on every mount and unmount, so the selected chip follows the mounted product.

**Unmounting.** `minimizeSPA()` returns to the last tab index. `closeSPA(tabId:)` drops the tab and minimizes only when that tab was mounted. The product game pill widget is detached while the product it counts down to is mounted.

## Adding a Tab

The bar has four fixed slots. `DSTabBarContainer` lays out exactly four slot views, so the count of tabs does not change the layout. To add a tab that fits in the existing four slots:

1. Add a case to `TabBarItem` with a localized `title` and an image.
2. Build its controller in `TabFactory.view(for:)`.
3. Add an entry to `AccessibilityID.Tab` so that `AccessibilityID.Tab.item(for:)` returns it.
4. Place the new slot in the presenter's slot list. Its position in the list is its position in the bar.

A fifth slot needs `DSTabBarContainer` to take a fifth view. Items past the fourth are not rendered, and a missing fourth item leaves a blank column.

## Gotchas

- **Tips on bar items do not show.** `TabBarTipController.anchorItem(for:)` returns nil for `.barItem` anchors, so only the status strip anchor can show a tip. The scan tip is anchored to `.action(.scan)`, which no bar slot uses.
- **Size the center panel from the chrome's width.** The hosting view is 0 wide when the chrome first lays out. A height computed from it starts the scan panel at the wrong size, and SwiftUI then routes the first taps on the embedded UIKit panel to its own host view, so the search field ignores them until the panel is reopened.
- **Two actions have no slot.** `TabBarAction.scan` and `TabBarAction.connectionStatus` have no bar slot. The center control and the shelf open them directly, so the branches for them in `handleAction` are not reached from a slot tap.
