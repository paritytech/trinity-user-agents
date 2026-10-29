# Tab Bar Container

The bottom tab bar is SwiftUI inside a thin UIKit shell. SwiftUI draws the bar row, the center control, the center panel and the shelf. UIKit keeps the three jobs that need a `UIViewControllerTransitionCoordinator` or a `UIView` hit test: the fold, the passthrough hit testing, and the resting bottom clearance that content is inset by.

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
| Fold drag decision | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldDecision.swift` |
| Panel state machine | `polkadot-app/Modules/MainTabBar/Chrome/TabBarPanelController.swift` |
| Panel kinds | `polkadot-app/Modules/MainTabBar/Chrome/TabBarPanelKind.swift` |
| Bar row view (SwiftUI) | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBarView.swift` |
| Bar row state | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBarViewModel.swift` |
| Shelf content (SwiftUI) | `polkadot-app/Modules/MainTabBar/Chrome/TabBarShelfView.swift` |
| Slots | `polkadot-app/Modules/MainTabBar/TabBarSlot.swift`, slot list in `MainTabBarPresenter.swift` |
| Slot-to-tab index mapping | `polkadot-app/Modules/MainTabBar/Chrome/TabBarSlotMap.swift` |
| Tab items | `TabBarItem` in `polkadot-app/Modules/MainTabBar/MainTabBarProtocols.swift` |
| Center panel height declaration | `polkadot-app/Modules/MainTabBar/Chrome/TabBarCenterContentHeight.swift` (`.fits(CGFloat)` or `.fill`) |
| Center panel content (scanner) | `polkadot-app/Modules/MainTabBar/ScanPanelViewController.swift` |
| Onboarding tips | `polkadot-app/Modules/MainTabBar/Chrome/TabBarTipController.swift`, `polkadot-app/Modules/MainTabBar/Tips/` |
| Floating widgets | `polkadot-app/Modules/MainTabBar/TabBarWidgetController.swift`, `MainTabBarFloatingWidgetStackView.swift` |
| Bar, center control, shelf container | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarContainer.swift`, `DSTabBarCenterControl.swift` |
| Bar outline, morph shape, geometry | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarShape.swift`, `DSTabBarMorphShape.swift`, `DSTabBarStyle.swift` |
| Bar material, dim layer | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarMaterial.swift`, `DSTabBarVariableBlur.swift` |
| Colours and type | `DSTabBarStyle.Palette` (token-less values only) in `DSTabBarStyle+Palette.swift` and `DSTabBarStyle.swift`. Themed colours and type come from DesignSystem tokens |
| Bar item and chip models | `polkadot-app/Modules/MainTabBar/Chrome/TabBarBarItem.swift` (`TabBarBarItem`), `TabBarShelfChip.swift` (`TabBarShelfChip`) |
| Bar icons | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarConnectionGlyph.swift`, `DSTabBarTabsGlyphView.swift` |
| Top status strip | `Packages/PolkadotUI/Sources/Modules/MainTabBar/ChainConnectionStatusBarView.swift` |
| SPA tab store and controller pool | `polkadot-app/Modules/Browser/SPABrowserCoordinator.swift`, `polkadot-app/Modules/Browser/Pool/SPAControllerPool.swift` |
| SPA chip view models | `polkadot-app/Modules/MainTabBar/SPATabChipViewModel.swift` |
| `topmostViewController` | `Packages/UIKitExt/Sources/UIWindow/UIWindow+keyWindow.swift` |
| `mainTabBarController` | `polkadot-app/Common/Extension/UIApplication+MainContainer.swift` |

## Bar Row

`DSTabBarContainer` lays out two slots left of the center hole and two right of it. The bar row takes four `TabBarSlot` values, in the order the presenter lists them.



The bar row is an accessibility container with the tab-bar trait.

**Center hole.** The notch is cut from the bar's top edge. Its bowl is sized from the center button, so the resting center button stands above the bar. The two inner slots stop at the shoulders of the bowl. `DSTabBarShape` cuts the same bowl from the bar. While the shelf is open, the part of the bowl above the row is cut as well, so the resting button keeps its ring.

**Selection pill.** `DSTabBarContainer` draws the selection pill behind the slot it gets from its `selectedSlot` parameter. The pill slides between slots. It snaps under Reduce Motion. The Apps slot never takes the pill. It reads as active through its colour.

- A tab slot selects a tab. Taps are plain taps. There is no drag across tabs.
- The Apps slot opens the shelf. It shows the open-product count and reads as active while the shelf is open or a product is mounted.
- `TabBarSlotMap` converts between bar position (item index) and tab position (tab index). Only tab indices cross into `TabBarContainer`.
- Titles appear under the icons only when the interactor enables labels (`setLabels(visible:)`).
- While the bar is folded, a slot tap only unfolds the bar. It does not select.

## Center Control and Center Panel

The center control, `DSTabBarCenterControl`, sits in the hole and shows the center button at rest. It draws the center panel when open. It is built from these parts:

- **Morph shape** (`DSTabBarMorphShape`). One shape for the resting button and the open panel. Its body rounds into a lobe under the bottom edge. The lobe narrows while the panel is narrow.
- **Scan frame** (`DSTabBarScanFrameShape`). The viewfinder brackets on the center button.
- **Materials** (`DSTabBarBarMaterial`, `DSTabBarCenterMaterial`). Glass for the bar row and for the control. They are separate surfaces, so each can tune its own intensity. Dark themes add the grey wash and the open fill. Light themes (read from `colorScheme`, which the app sets from the theme) use a stronger blur with neither.
- **Dim layer** (`DSTabBarDimLayer`). Dims the screen behind an opening center panel.
- **Grabber.** A handle on the open panel. It shows with the content.

Every width, height and radius is read from one progress value. The shape is valid at any point of a drag.

**Gradient rule.** Gradient strokes on a resizing shape must be wrapped in a drawing group. SwiftUI rasterises gradient strokes on the CPU, so an unwrapped stroke costs a full frame on every morph step.

**States.** The center panel has two states, and both use the same panel:

- **Scan state.** The camera is live. The center button opens it on a tap.
- **Search state.** The search field has focus. The panel fills the room above the bar. The bar row fades out while the panel stays.

Scan and Search change the layout with the same open motion. A Scan ↔ Search change while open does not close the panel.

**Gestures.** A tap opens the Scan state. An upward swipe on the button opens the panel into the Search state. A downward drag or a tap closes it. A flick decides the end of a drag. Without a flick, the halfway point decides. Content drags from the open panel never open the Search state.

**Settling.** The control reports open or closed to the chrome (`onSettled`) when its own settle completes. It never reports during a drag. A UIKit-driven move through `TabBarBarViewModel.setCenterPanelOpen` reports at the end of its animation. A drag past open stretches the panel. A drag past closed clamps at closed. Both commit only on release and settle. A cancelled drag settles to the nearer end. A grab during a settle starts from the on-screen progress.

**Content visibility.** During a drag, the content (the scanner) and the grabber appear only after the drag covers most of the travel. A settle open shows them at once. They ride the panel's top edge while the shape crops them. On a close, the shape moves first. The content and the grabber fade out with the close's final part. The Search state's results list fades in after a state change. Under Reduce Motion, the progress snaps and the content cross-fades. A close waits for that cross-fade before the shape snaps.

**Content lifetime.** The chrome builds the content the first time the control shows it. The control removes the content when it removes the panel.

**Panel height.** The panel declares its height rather than measuring the content. `ScanPanelViewController.declaredContentHeight()` returns a `TabBarCenterContentHeight`:

- `.fits(CGFloat)` for a fixed height. `ScanPanelViewLayout.declaredHeight` owns the layout constants.
- `.fill` while search has focus. The chrome caps the panel at `availablePanelHeight`, the room between the bar row and the bottom of the status strip.

The chrome takes the width from `DSTabBarStyle.rowWidth(inAvailableWidth:)` of its own view width, not from the hosting view. The hosting view is laid out after the chrome. The chrome recomputes the height on layout and whenever the content reports a change. Measuring a live camera would start a second capture session, so the height is never measured from it.

**Search focus.** The scan panel's search field is always present. Focusing it puts the panel in the Search state. `ScanPanelViewController.onSearchFocusChanged` reports focus. `MainTabBarViewController` passes it to `TabBarBottomChromeController.setBarHidden(_:)`. `DSTabBarContainer`'s `isBarHidden` fades the bar row out while the panel stays. The bar returns when focus ends. Closing or folding the panel also ends focus. A pan that begins on the search field stays with the field and does not move the panel.

**Swipe into Search.** This flow opens the Search state from a swipe on the center button.

1. `DSTabBarCenterControl` takes two optional callbacks. `onSwipeOpenBegan` fires once per drag. It fires when a drag on the button that began short of fully open moves up past the tap slop. Content drags from the open panel never fire it. The package does not know what the callback means.
2. `TabBarBottomChromeController` forwards both callbacks from the model. It passes nil when the app sets no handler.
3. `MainTabBarViewController` holds `isSearchSwipePending`. The scan panel controller may not exist yet when the swipe begins. The panel declares `.fill`, so the travel runs to the filled panel's top.
4. `ScanPanelViewController.setSearchPending(_:)` shows the Search state layout without focus and keeps the camera off. The bar keeps the Scan state look.
5. `onSwipeOpenLanding` fires when the release settles open, partway through the settle. It fires at once under Reduce Motion. The chrome commits `.content(.scan)` first. `currentKeyboardLift` then sees the open scan panel when the keyboard frame changes. The host then calls `focusSearch()`, which runs the normal focus path.
6. `setCenterPanelOpen(true)` returns early because the progress is already open. The release animation is not restarted.
7. A close or a settle to closed drops the swipe and calls `setSearchPending(false)`. The panel returns to the Scan state's layout. The chrome reuses the content controller until a committed close. A swipe that never landed open never commits one.

**Keyboard.** The keyboard lift has one owner, `TabBarFoldController.keyboardLift`, which the fold transform carries. `TabBarBottomChromeController` recomputes it on each keyboard frame change, using the keyboard's own duration and curve. It also recomputes on every panel open and close. While the scan panel is open, the chrome lifts until the keyboard covers the center button's foot by `keyboardButtonOverlap`. Any other keyboard, or a closed panel, leaves the chrome at rest. The lift reduces `availablePanelHeight` by the same amount.

**Search field and list.** The scan panel's field (`DSSearchRowView`), and its scan button share the field material, `DSTabBarFieldMaterialView`. The field switches its icon position between the Scan state and the Search state (`setSearchFocused`). The results list is shared with the full-screen Search Contact screen. Colours are DesignSystem theme tokens that follow all five themes. `DSTabBarFieldMaterialView` and `DSSearchRowView` re-apply their resolved colours when `DSThemeTrait` changes. `DSTabBarStyle.Palette` keeps only the values with no token: the glass stroke, the glass wash, the open fill and the dim wash. Search type uses DS typography tokens, so the bar has no type style of its own. `DSSearchAvatar` shows the contact's image, otherwise `DSLetterAvatar` at `.s40`. The shared `DSAvatar` stays untouched.

## Closing and Capture

**Prepare before close.** Before the close animation starts, the chrome calls `ScanPanelViewController.prepareForClose()`. It stops capture, then cancels the search. Ending search focus therefore does not restart the camera.

- `closeCenterPanel` calls it before it moves the progress. The control removes content a drag already hid as soon as the progress reaches 0. That removal ends search focus.
- `applyCommittedPanel` calls it again. The second call does nothing.
- The control never removes content mid-drag. A drag held at closed leaves the removal to its settle, after `onSettled`.
- While search has focus, `setCaptureActive(true)` from a settle that re-reports open leaves the camera off.

**Preview attach.** Attaching or detaching `AVCaptureVideoPreviewLayer.session` on a running session reconfigures it on the main thread. This is slow enough to show as a stall. The rules that follow from it:

- Pausing capture (search focus, close) only hides the preview and stops the session. The layer stays attached, so a restart does not reconfigure.
- `EmbeddedQRScannerViewController` detaches the layer once, when its view disappears. It detaches at once on a stopped session. Otherwise it waits for `AVCaptureSession.didStopRunningNotification`. A restart cancels that wait.
- `QRCaptureService` calls `stopRunning` only on a running session. Queued extra stops from the presenter's and the service's `deinit` could land inside the detach, which throws.
- The first attach happens before the session starts. `QRCaptureService` calls `qrCapture(service:willStart:)` through `DispatchQueue.main.sync` before `startRunning`. The view attaches the stopped session and does not show it. After the start, `configureVideoLayer` finds the layer attached and only fades the preview in.
- Nothing on the main thread may wait synchronously on the capture queue. A `main.sync` that waits on that queue deadlocks.

**Opening from code.** `ModuleNavigator.openScanPanel()` sets `.content(.scan)`, and the control follows through `setCenterPanelOpen`. Closing the panel from any source closes the control.

**Reduce Motion.** The geometry snaps, and only the content cross-fades.

## Haptics and Accessibility

**Haptics.** The PolkadotUI components play sensory feedback next to the gestures that trigger it. `DSTabBarCenterControl` owns the open and close feedback for the control. `DSTabBarContainer` owns the feedback for a bar-hidden change while the panel is open. The app chrome plays no haptics. The landing focus of a swipe into Search passes a flag that mutes the container's bar-hidden feedback. Each slot tap plays a selection feedback from a simultaneous gesture in the bar row, including a reselect.

**Accessibility.** VoiceOver sees the center control as a button. The caller passes its label. The app passes the `QR scanner` string. When the panel is open, the escape action closes it.

## Shelf

The Apps slot opens the shelf, which grows upward from the bar row. Its height is the content height, capped at the space between the bar row and the top safe area. Beyond the cap the grid scrolls.

- The top row holds a connection status icon and a settings icon. Neither is a tab. Connection status opens as a page sheet. Settings opens as a modal navigation stack. Both close the shelf first. `MainTabBarWireframe` builds that stack through the injected `TabFactoryProtocol`. Opening device details while the Settings stack is the topmost presented controller pushes onto it.
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

UIKit stops hit testing at a view's bounds. `TabBarBarHostView` is sized to the full room above the bar at rest, which holds any open panel. It never resizes for a panel. Its `liveHeight` sets the region that accepts touches. That region is the bar row plus a rise. The row stays at the bottom.

The rise is the taller of two values:

- the resting rise, which keeps the resting center button touchable,
- the open panel's rise. For the center panel this is the panel's declared rise. For the shelf it is the shelf's content height, capped at the available height.

The rise is set when the panel state commits and is not animated. The live region is set as soon as the bar is installed. The SwiftUI springs animate what is drawn inside it.

`TabBarChromePassthroughView` claims a touch in only two cases: the folded grab zone, and the dismiss area while a panel is open. Every other touch goes to the content behind the chrome. `TabBarChromeSurfaceView` returns nil for hits on its own empty area. `TabBarBarHostView` accepts touches in its live region.

The fold pan recognizer starts only on the bar row outside the center hole, and only for a drag that leads horizontally. It does not start on a tab root or while the bar is `hidden`.

## Fold

The three states are `shown`, `folded` and `hidden`.

- **Shown.** The bar sits at the bottom of the screen.
- **Folded.** The bar slides to the leading edge and leaves a sliver. A grab zone on the passthrough view restores it on tap.
- **Hidden.** The bar slides fully off the leading edge. There is no sliver and no grab zone, so tab navigation is not available on that screen. The only conformer to `TabBarChromeVisibilityCustomizing` in the app is `ChatViewController`.

`TabBarVisibilityPolicy.state(isTabRoot:derived:override:)` resolves the state in this order:

1. A tab root is always `shown`. A tab root never folds.
2. A per-screen user override (`folded` or `shown`) wins.
3. Otherwise the derived state applies.

The derived state comes from `TabBarHiddenPolicy.deriveFoldState(in:showing:)`. It is `hidden` when the target opts out. It is `folded` when any controller above the root, up to the target, sets `hidesBottomBarWhenPushed`. The root is excluded, so a tab root that sets the flag does not fold.

Overrides are stored per screen, weakly keyed on the navigation screen, so folding one pushed screen does not affect another. The fold controller ignores override requests while the bar is `hidden`.

**Interactive pops.** The transition coordinator drives the fold. `setInteractiveTarget` runs inside `animate(alongsideTransition:)`. When the interaction ends, the chrome commits the incoming screen's state. If the pop was cancelled, it commits the staying screen's state, rebuilt with `TabBarHiddenPolicy.stackAfterCancelledPop`.

**Animation.** A spring takes the gesture's velocity as its initial velocity.

`TabBarFoldDecision` classifies the end of a horizontal drag on the bar row.

- From `shown`, a fast leftward flick folds the bar. Any other drag reads as a selection, which the bar takes no action on, so the bar stays shown.
- From `folded`, a tap or a fast rightward flick unfolds.
- Otherwise the drag decides by how far it passes the fold distance. It unfolds when it passes far enough. Otherwise it settles back to folded.

## Clearance and Safe Area

Two bottom insets stack, and they go on different controllers so they compose.

| Inset | Applied to |
|---|---|
| Bar clearance | the tab's `UINavigationController`, or the tab controller itself when there is none |
| Widget height | the top screen's controller, or added to the tab controller when there is no navigation controller |

The two change at different rates. The navigation controller is stable for the tab's lifetime, and the widget height follows the top screen. `updateContentSafeAreaInset()` zeroes the controller it inset last before it writes the new one.

- `restingBarHeight` is the bar's resting height on a tab root (`DSTabBarStyle.restingHeight`). Elsewhere it is the raw bottom safe area.
- `restingBottomClearance` is `restingBarHeight` minus the bottom safe area, floored at zero.
- Clearance is contributed only on a tab root. A pushed screen that does not set `hidesBottomBarWhenPushed` reserves no space. The bar floats over its bottom content and takes touches in its capsule. Such a screen should either set the flag or keep interactive content clear of the bottom.
- The shelf and the center panel overlay content. They do not change clearance.

**Recalculation.** Three events recompute the insets from `container.selectedController`: the container's safe area changes, the chrome's safe area changes, and a tab mounts. Re-deriving from the selected controller stops a late `didShow` from a background tab pinning insets to a screen that is not on screen.

**Fold inputs.** `isTabRoot`, the derived state and the screen change only through `applyChrome(_:animatingAlongside:)`. The inset path reads them but never writes them.

**No window.** `updateWidgetLayout` returns while the view has no window, so a `.fullScreen` presentation elsewhere does not produce bad insets. The last applied insets hold until the view reattaches.

## Top Status Strip

A fixed-height strip sits at the top of the container. It shows one ring per connection row. Each ring's accessibility label is the chain title and its state. No chain name is drawn. The strip has no tap handling, does not fold, and does not hide.

- `installStatusBar()` sets `additionalSafeAreaInsets.top` to the strip height once, in `viewDidLoad`. Every mounted navigation controller passes that inset to the screens it pushes, so a pushed screen's top safe area already includes the strip.
- The strip paints no fill. The container's background shows behind the system status bar and the strip.
- `installStatusBar()` runs before `installChromeController()`, so the bar stays above the strip. Mounted children insert at index 0, so they sit below it.

## Mounting and Re-tap

`TabBarContainer` mounts one child at a time, full bleed into the container's view. A tab switch cross-fades. The outgoing view is removed when the fade completes. Non-animated mounts skip the fade.

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

`presentProduct(page:)` is the one-shot path for funding products. It presents a page sheet. The product never enters the tab store, so closing it leaves no chip.

**Chips.** `MainTabBarInteractor` sends the open tabs to the presenter (`didReceiveSPATabs`). The presenter maps them through `SPATabChipViewModelFactory`, and the view passes them to the chrome as `TabBarShelfChip`. `applyChips()` runs on every mount and unmount, so the selected chip follows the mounted product.

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
- **Size the center panel from the chrome's width.** The hosting view has zero width when the chrome first lays out. A height computed from it starts the scan panel at the wrong size. SwiftUI then routes the first taps on the embedded UIKit panel to its own host view, so the search field ignores them until the panel is reopened.
- **Two actions have no slot.** `TabBarAction.scan` and `TabBarAction.connectionStatus` have no bar slot. The center control and the shelf open them directly, so the branches for them in `handleAction` are not reached from a slot tap.
