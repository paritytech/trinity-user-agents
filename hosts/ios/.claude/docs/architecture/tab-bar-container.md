# Tab Bar Container

The bottom tab bar is SwiftUI inside a thin UIKit shell. SwiftUI draws the bar row, the center control, the center panel and the shelf. UIKit keeps the three jobs that need a `UIViewControllerTransitionCoordinator` or a `UIView` hit test: the fold, the passthrough hit testing, and the resting bottom clearance that content is inset by.

`MainTabBarViewController` is a plain `UIViewController`. It mounts one child at a time, owns the top status strip, and writes the bottom insets on the tab and content controllers.

## Model in brief

- The bar row has four fixed slots around a center hole. Slots select tabs or open the shelf.
- The center control sits in the hole and morphs into the center panel. Every size is read from one progress value between 0 and 1.
- One value, `TabBarPanelKind?`, says which panel is open: `.shelf` or `.content(.scan)`. The shelf and the center panel cannot both be open.
- One value, `TabBarVisibilityState` (`shown`, `tucked`, `hidden`), says where the bar sits. The fold controller animates it.

## Ownership

```
MainTabBarViewController            container, status strip, widget and inset layout, SPAHosting
├── TabBarContainer                 mounts one tab or one SPA
├── ChainConnectionStatusBarView    20 pt strip, informational only
└── TabBarBottomChromeController    bar, panels, fold, outside tap, bar items
    ├── TabBarFoldController        visibility state, auto-hide, hide progress via DockletMotion,
    │                               CADisplayLink; sole writer of the surface transform
    └── TabBarChromePassthroughView full-bleed, hit tests only the grab zone and the outside-tap area
        ├── TabBarChromeSurfaceView fold transform target, bar layout
        │   └── TabBarBarHostView   accepts touches in the bar row and the open panel's rise
        │       └── UIHostingController<TabBarBarView>
        │                           SwiftUI bar row, center control, shelf
        └── Edge glow view          TabBarEdgeGlowView, right-edge glow of a tucked bar, outside the surface transform
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
| Edge glow band, boost and shimmer | `polkadot-app/Modules/MainTabBar/Chrome/TabBarEdgeGlowView.swift` |
| Fold transform target, bar width | `polkadot-app/Modules/MainTabBar/Chrome/TabBarChromeSurfaceView.swift` |
| Fold state machine | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldController.swift` |
| Fold state resolution | `polkadot-app/Modules/MainTabBar/Chrome/TabBarVisibilityPolicy.swift` |
| Hide progress, drag and release, settle dispatch, quiet hide and content fade | `polkadot-app/Modules/MainTabBar/Chrome/DockletMotion.swift` (pure value type) |
| Settle spring and landing rule | `polkadot-app/Modules/MainTabBar/Chrome/DockletSpring.swift` (pure value type) |
| Release velocity sampling | `polkadot-app/Modules/MainTabBar/Chrome/DockletDragSamples.swift` (pure value type) |
| Reduce Motion fade (0.22 s) | `polkadot-app/Modules/MainTabBar/Chrome/DockletReducedFade.swift` (pure value type) |
| Auto-hide blockers, idle countdown, outside-touch tuck | `polkadot-app/Modules/MainTabBar/Chrome/DockletAutoHide.swift` (pure value type) |
| Window touch observer that feeds the auto-hide | `polkadot-app/Modules/MainTabBar/Chrome/DockletTouchObserver.swift` |
| Docklet tip rules: which tip a tuck shows, fling distance | `polkadot-app/Modules/MainTabBar/Chrome/DockletTipPolicy.swift` (pure value type) |
| Docklet tip lifecycle: appear delay, seen flags, restore, fly-off | `polkadot-app/Modules/MainTabBar/Chrome/DockletTipController.swift` |
| Docklet tip view: layout, slide in, drag, fly-off | `polkadot-app/Modules/MainTabBar/Chrome/DockletTipView.swift` |
| Session nudge timing and inset | `polkadot-app/Modules/MainTabBar/Chrome/DockletNudge.swift` (pure value type) |
| Fold controller outputs to the chrome | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldController+Outputs.swift` |
| Delayed main-actor task | `polkadot-app/Modules/MainTabBar/Chrome/Task+Delayed.swift` |
| Bar frame, pivot, slide, Edge glow and Grab zone rects, plain pose | `polkadot-app/Modules/MainTabBar/Chrome/DockletGeometry.swift` (pure value type) |
| Edge glow alpha profile, blur and feather stops | `polkadot-app/Modules/MainTabBar/Chrome/DockletGlowProfile.swift` (pure value type) |
| Grab aim: progress and pose from a finger in the Grab zone | `polkadot-app/Modules/MainTabBar/Chrome/DockletAim.swift` (pure value type) |
| Held pose, direction lock | `polkadot-app/Modules/MainTabBar/Chrome/DockletHeldPose.swift` (`DockletHold`, `DockletDirectionLock`, `DockletPose`) |
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
- While the bar is tucked, no slot is reachable. A tap in the grab zone restores the bar.
- During a quiet hide, the icons and labels fade with the bar. See Quiet hide under Fold.

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

**Haptics.** The PolkadotUI components play sensory feedback next to the gestures that trigger it. `DSTabBarCenterControl` owns the open and close feedback for the control. `DSTabBarContainer` owns the feedback for a bar-hidden change while the panel is open. The app chrome plays two impacts, and only for a tuck or restore the user starts: a heavy impact when the bar lands tucked, and a medium impact when it lands shown. `TabBarFoldController` plays them through `UIImpactFeedbackGenerator`. A settle the app starts, or one that keeps the same rest, plays nothing. An auto-hide tuck plays nothing either. A grab released back to tucked plays nothing. A grab released to shown plays the medium impact. While a drag moves the bar, `TabBarFoldController` also plays a selection tick (`UISelectionFeedbackGenerator`) when projected progress crosses 0.54 upward or 0.46 downward. The landing focus of a swipe into Search passes a flag that mutes the container's bar-hidden feedback. Each slot tap plays a selection feedback from a simultaneous gesture in the bar row, including a reselect. Reduce Motion does not change the chrome's impacts.

**Accessibility.** VoiceOver sees the center control as a button. The caller passes its label. The app passes the `QR scanner` string. When the panel is open, the escape action closes it.

**Tucked bar and VoiceOver.** While the bar is tucked, or settling toward tucked, the Edge glow is its one accessibility element, labelled "Show tab bar" (`Common.tabBarShowAccessibilityLabel`). Activating it restores the bar as the user's restore, with a medium haptic. The bar's own elements are hidden from VoiceOver while it is tucked. A `layoutChanged` notification moves focus to the glow on tuck and to the bar on restore. Auto-hide never runs while VoiceOver is on. Turning VoiceOver off restarts the 3 s countdown from that moment.

**Reduce Motion.** Read live, and observed for changes. Any tuck or restore that is not a finger drag (release, tap, grab release, tip, auto-hide, or a policy change) fades instead of swinging. The fade takes 0.22 s (`DockletReducedFade`). Only the icons, labels and center glyph fade. The glass goes straight to its end pose: a restore shows it at once, and a tuck holds it in place while the content fades, then places it tucked. The Edge glow follows the content fade. A drag still follows the finger. Turning Reduce Motion on during a settle ends that settle at its destination. The first-hide nudge does not play. A move between `shown` and `hidden` is written without animation.

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

`TabBarChromePassthroughView` claims a touch in only two cases: the grab zone, and the dismiss area while a panel is open. The grab zone is 47 pt wide on the right edge and 200 pt tall from the top of the tucked bar. Its top stays at least 56 pt below the screen top, and its bottom at least 8 pt above the screen bottom. The height shortens only when the bottom clamp forces it. It is set only while the bar is tucked or settling toward tucked. A tap in it restores the bar through the chrome's outside-tap recognizer. The root reports when a touch that began in the grab zone starts, and releases the Edge glow's held look only when that touch ends or is cancelled. Other touches do not release it. Every other touch goes to the content behind the chrome. `TabBarChromeSurfaceView` returns nil for hits on its own empty area. `TabBarBarHostView` accepts touches in its live region.

The fold pan recognizer starts on any part of the bar row, the center button and the center hole included, for a drag that `DockletDirectionLock` accepts as sideways. It does not start while the bar is `hidden` or a panel is open. It refuses touches in the Grab zone.

A second pan sits on the chrome root view (`installGrabRecognizer`, `isGrabPan`). It receives only touches inside the Grab zone, in any direction, with no direction lock. The two pans never share a touch. The grab pan does not cancel the root view's touches. A tap with no movement in the Grab zone still restores the bar through the outside-tap path.

## Fold

Design vocabulary and decisions for the Docklet work live in `.scratch/docklet-r2/` until they move to `CONTEXT.md` and `docs/adr/`.

The three states are `shown`, `tucked` and `hidden`. The bar's position between shown and tucked is one value, the hide progress: 0 when shown, 1 when tucked. `DockletMotion` (`polkadot-app/Modules/MainTabBar/Chrome/DockletMotion.swift`) owns that value. It is a pure value type with no UIKit and no clock, so the caller passes in time.

- **Shown.** The bar sits at the bottom of the screen.
- **Tucked.** The bar turns a quarter about a pivot 14 pt past its right end, at its vertical center, with the leading end up. It then slides out to the right by (gap to the screen edge + 60 pt) × progress³. An Edge glow, 24 pt wide and 200 pt tall from the top of the tucked bar, sits on the right edge, outside the bar's transform. Its top and height follow the grab zone's clamps. Its opacity follows the hide progress while a finger drags the bar or it settles toward tucked. A settle toward shown fades it out over 320 ms. While a finger holds the grab zone, a brighter copy and a shimmer light it up and it widens to 1.08 from the screen edge. Release fades that look out over 320 ms. Reduce Motion keeps the fades and skips the widening and the shimmer's movement.
- **Hidden.** The bar slides fully off the leading edge. There is no Edge glow and no grab zone, so tab navigation is not available on that screen. The only conformer to `TabBarChromeVisibilityCustomizing` in the app is `ChatViewController`.

`TabBarVisibilityPolicy.state(derived:override:)` resolves the state in this order:

1. A derived `hidden` (chat) wins over everything.
2. A user override (`tucked` or `shown`) wins.
3. Otherwise the derived state applies.

A tab root follows the same order and can be tucked by a user override. Tab-root bottom clearance does not change when the bar tucks, and it does not follow the motion.

The derived state comes from `TabBarHiddenPolicy.deriveFoldState(in:showing:)`. It is `hidden` when the target opts out. It is `tucked` when any controller above the root, up to the target, sets `hidesBottomBarWhenPushed`. The root is excluded, so a tab root that sets the flag does not tuck. A mounted product also derives `tucked`. The grab zone can bring the bar back over it.

`TabBarFoldController` holds one user override, not one per screen: `none`, `shown` or `tucked`. A user tuck or restore sets it, and so does an auto-hide tuck. The value lives in memory, so a relaunch starts with the bar shown. The fold controller ignores override requests while the bar is `hidden`.

**Navigation.** When the screen changes (push, back, tab switch, or an interactive transition resolving), the new screen starts with `tucked` if the bar was tucked. Otherwise it starts with no override, so its derived state decides. A tucked bar therefore stays tucked across navigation, including when the user leaves a product. Leaving a chat shows the bar. A mounted product or a `hidesBottomBarWhenPushed` screen still tucks.

**Tucked and hidden.** Moving between `tucked` and `hidden` writes the end pose without animation, because the bar is off screen at both ends.

**Interactive pops.** The transition coordinator drives the fold. `resolvedState(for:)` gives the incoming screen's state, using the carried override. `setInteractiveTarget` moves the bar toward it inside `animate(alongsideTransition:)`. When the interaction ends, the chrome commits the incoming screen's state. If the pop was cancelled, the staying screen's state is rebuilt with `TabBarHiddenPolicy.stackAfterCancelledPop`.

**Drag.** A horizontal pan that starts on the bar follows the finger live. `DockletDirectionLock` decides the pan's direction once it has moved 10 pt. At rest, sideways means between 30° up and 45° down. A bar caught mid-settle accepts any pan within 60° of horizontal. A touch on a moving bar catches it where it is. A drag that starts near the right edge needs less travel to hide the bar. A release that changes the rest records the user override.

**Held pose.** `DockletHold` owns the pose while a finger holds the bar. The bar turns half as far as its plain pose, about the point under the finger, and follows the finger. It leads the finger towards the right edge by up to 56 pt, keeping the finger 14 pt inside the bar's leading end. Past the ends, progress rubber-bands by at most 0.25. The bar resists past Shown (40 pt) and above the tucked bar's top (40 pt). It never sinks below the screen bottom unless the finger drags below it (36 pt).

**Release.** Velocity comes from the last 100 ms of samples. A final pause is ignored. The bar tucks when projected progress (progress + velocity × 0.1 s) is at least 0.5. A drag that came back more than 30% of its furthest travel returns to the rest it started from.

**Bringing it back.** A touch in the Grab zone catches a tucked bar, or one still settling toward tucked, and turns it to point at the finger. `DockletMotion.beginAim` starts the drag. Progress is the finger's angle from the pivot over a quarter turn. The pivot slides out with progress, so the angle is solved in two passes per frame (`DockletAim`). Low and far in gives shown. High at the edge gives tucked. The grab's progress and pose blend out over the first 80 pt of finger travel, so catching the bar does not make it jump. Release uses the same rules as a tuck drag, plus one: a leftward finger speed above 350 pt/s over the last 100 ms restores the bar, however short the drag.

**Animation.** A release starts a spring (`DockletSpring`) that takes the release velocity as its initial velocity. Faster than 4 progress per second, the spring is under-damped (ratio 0.78). Otherwise it is critically damped. A `CADisplayLink` in `TabBarFoldController` drives it. The held offset unwinds on the same clock. The settle lands once it is near rest, after 80 ms, and at 1.4 s at the latest. Under Reduce Motion, a settle fades instead (see Reduce Motion under Haptics and Accessibility). `TabBarFoldController` is the only writer of the chrome surface transform. Outside Reduce Motion, a move between `shown` and `hidden` uses a plain UIKit spring animator, because the display link drives only the shown and tucked poses. Under Reduce Motion that move is written without animation. A swap between `tucked` and `hidden` writes the end pose without animation.

**Auto-hide.** A shown bar tucks itself without a drag, in two cases:

- The first touch outside the Docklet tucks it at once.
- Otherwise it tucks 3 s after the last touch or hardware key press.

The Docklet is the bar, the open panel and the Grab zone. The Edge glow strip does not count while the bar is shown. A touch on the Docklet restarts the 3 s countdown and cancels a pending outside-touch tuck. Further outside touches do not restart that tuck. The tuck is stored as the user override.

Blockers hold the tuck: an open panel, the keyboard, a drag or settle in progress, a bar that is not shown, a Docklet tip on screen (`tipShown`), and VoiceOver (`voiceOver`). While a blocker holds, an outside touch starts no tuck. Clearing the last blocker restarts the 3 s countdown from that moment.

`DockletAutoHide` is the pure policy and reads no clock, so the caller passes in times. `DockletTouchObserver` is a window recognizer. It reports each touch that begins and never recognizes, cancels or delays one. `TabBarFoldController` feeds both and schedules one wake-up for the next deadline.

**Quiet hide.** The auto-hide tuck uses the quiet settle, a critically damped spring with a 0.42 s period and no haptic. The first frame only starts its clock, so motion begins on the second frame.

Only the content inside the glass fades: the icons, the labels and the centre glyph. Its opacity is `1 − 1.12 × progress^1.15`, floored at zero, so it is gone from about progress 0.906. The glass and its ancestors never take alpha, because alpha on a visual effect view breaks the effect. `TabBarBarView` and `DSTabBarCenterControl` (`glyphOpacity`) apply the value.

The fade stays while the bar is tucked. A drag, a grab or a return to shown clears it.

**Nudge.** The first tuck that lands in a launch, user or auto-hide, starts a nudge 700 ms later. The bar moves 8 pt in from the edge and back over 400 ms. A drag or a restore cancels it. Under Reduce Motion it does not play. The arm is held in memory, so each launch plays the nudge at most once.

## Tips

The Docklet tips are custom views beside the Edge glow, not TipKit, because a user can fling them away and TipKit does not follow a drag. Each tip shows once per device. Its seen flag is a `SettingsManager` key: `dockletTuckedTipShown` or `dockletAutoHiddenTipShown`.

- **Tucked tip.** Shows 420 ms after the first swipe tuck lands.
- **Auto-hide tip.** Shows 420 ms after the first auto-hide tuck lands. It also marks the tucked tip as seen.
- A tip is skipped while a TipKit popover is presented. It stays unseen, so a later tuck can show it.
- A fold drag or a grab that begins before the tip appears cancels it.
- While a tip shows, it blocks auto-hide (`tipShown`).
- A tap, or a release more than 56 pt from rest, restores the bar as the user's own restore, with a medium haptic. A shorter release snaps the tip back.
- Any other way the bar leaves the tucked state flies the tip off to the right over 320 ms.

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
4. `applyChrome(.spa(controller))` sets `isTabRoot: false` and a derived state of `tucked`. The bar tucks and contributes no clearance.

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
