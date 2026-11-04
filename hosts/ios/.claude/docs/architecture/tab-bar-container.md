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
    ├── dimHost                     UIHostingController<TabBarDimView>, full-screen scrim below the surface
    ├── TabBarFoldController        visibility state, auto-hide, hide progress via DockletMotion,
    │                               CADisplayLink; sole writer of the surface transform
    └── TabBarChromePassthroughView full-bleed, hit tests only the grab zone and the outside-tap area
        ├── TabBarChromeSurfaceView fold transform target, bar layout
        │   └── TabBarBarHostView   accepts touches in the bar row and the open panel's rise
        │       └── UIHostingController<TabBarBarView>
        │                           SwiftUI bar row, center control, shelf; reads TabBarBarViewModel
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
| Docklet motion core: hide progress, settle (spring or Reduce Motion fade), release velocity, content fade target | `polkadot-app/Modules/MainTabBar/Chrome/DockletMotion.swift`, `DockletSpring.swift`, `DockletReducedFade.swift`, `DockletDragSamples.swift` (pure value types) |
| Path: pose, hold, aim, direction lock, Edge glow and Grab zone rects | `polkadot-app/Modules/MainTabBar/Chrome/DockletPath.swift`, `DockletDrawbridge*.swift`, `DockletHeldPose.swift`, `DockletAim.swift`, `DockletDirectionLock.swift`, `DockletGeometry.swift` (pure value types) |
| Auto-hide and session nudge | `polkadot-app/Modules/MainTabBar/Chrome/DockletAutoHide.swift`, `DockletTouchObserver.swift`, `DockletNudge.swift` |
| Edge glow profile | `polkadot-app/Modules/MainTabBar/Chrome/DockletGlowProfile.swift` (pure value type) |
| Fold controller outputs and helpers | `polkadot-app/Modules/MainTabBar/Chrome/TabBarFoldController+Outputs.swift`, `Task+Delayed.swift`, `CGFloat+Easing.swift` |
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
| Tip presenter: one popover at a time, Docklet tuck parameters, outside-tap dismissal | `polkadot-app/Modules/MainTabBar/Chrome/TabBarTipController.swift` |
| Tip types, rules, parameters, step list | `polkadot-app/Modules/MainTabBar/Tips/TabBarTipDefinitions.swift`, `TabBarTips.swift` |
| Tip step (anchor, `onShow`), anchors, popover body | `polkadot-app/Modules/MainTabBar/Tips/TabBarTipStep.swift`, `TabBarTipAnchor.swift`, `TabBarTipViewStyle.swift` |
| First eligible tip step | `polkadot-app/Modules/MainTabBar/Tips/TabBarTipOrderedSequence.swift`, `TabBarTipSequenceProtocol.swift` |
| Floating widgets | `polkadot-app/Modules/MainTabBar/TabBarWidgetController.swift`, `MainTabBarFloatingWidgetStackView.swift` |
| Bar, center control, shelf container | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarContainer.swift`, `DSTabBarCenterControl.swift` |
| Centre glyph, grabber, rim | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarCenterGlyph.swift` |
| Content fade (icons and labels opacity, read by leaf views) | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarContentFade.swift` |
| Drag input for centre panel content (`centerPanelDrag`) | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarCenterDrag.swift` |
| Selection pill (`dsTabBarSelectionPill`) | `Packages/PolkadotUI/Sources/Components/DSTabBarShell/DSTabBarSelectionPill.swift` |
| Full-screen dim scrim host | `polkadot-app/Modules/MainTabBar/Chrome/TabBarDimView.swift` |
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

`DSTabBarContainer` lays out two slots left of the center hole and two right of it. The bar row takes four `TabBarSlot` values, in the order the presenter lists them. Each slot is a `TabBarSlotView(index:model:)`, and each shelf cell takes its chip and the model. They call `TabBarBarViewModel` handlers at tap time, so no closures are passed to them.



The bar row is an accessibility container with the tab-bar trait.

**Center hole.** The notch is cut from the bar's top edge. Its bowl is sized from the center button, so the resting center button stands above the bar. The two inner slots stop at the shoulders of the bowl. `DSTabBarShape` cuts the same bowl from the bar. While the shelf is open, the part of the bowl above the row is cut as well, so the resting button keeps its ring.

**Outline and notch.** `DSTabBarShape` is the outline, the border and the clip that cuts the notch. On iOS 26 and later the glass is a plain rounded rect and the clip cuts the notch, because Liquid Glass puts a notch in the wrong place while the Docklet is rotated.

**Selection pill.** `DSTabBarContainer` draws the pill behind the slot at its `selectedSlot`. The pill slides between slots, and snaps under Reduce Motion. `TabBarBarViewModel.selectedSlot` takes that slot from `TabBarSelectionPlacement`:

- A tab: that tab's slot.
- A mounted product with the shelf closed: the Apps slot.
- A mounted product with the shelf open: no bar slot. The pill draws behind that product's cell in the shelf (`dsTabBarSelectionPill(isSelected:)`), so it is drawn once.

- A tab slot selects a tab. Taps are plain taps. There is no drag across tabs.
- The Apps slot opens the shelf. It shows the open-product count and reads as active while the shelf is open or a product is mounted.
- `TabBarSlotMap` converts between bar position (item index) and tab position (tab index). Only tab indices cross into `TabBarContainer`.
- Titles appear under the icons only when the interactor enables labels (`setLabels(visible:)`).
- While the bar is tucked, no slot is reachable. A drag from the grab zone restores the bar. Opening the Scan panel from a screen also restores it (`openPanelShowingDocklet`, used by "add contact" in Contacts), because the panel lives in the Docklet. A tap on the Edge glow, a Slot or a tip never restores it.
- During a quiet hide, the icons and labels fade with the bar. See Quiet hide under Fold.

## Center Control and Center Panel

The center control, `DSTabBarCenterControl`, sits in the hole and shows the center button at rest. It draws the center panel when open. It is built from these parts:

- **Morph shape** (`DSTabBarMorphShape`). One outline for the resting button and the open panel. Its body rounds into a lobe under the bottom edge. Below the card width the outline eases toward a circle, so the resting button is a circle.
- **Glyph** (`DSTabBarCenterGlyph`, in `DSTabBarCenterGlyph.swift`). The viewfinder brackets (`DSTabBarScanFrameShape`) sit in the lobe in both states. The glyph shrinks and drops as the panel opens. It applies the content fade itself (see Fold).
- **Materials** (`DSTabBarBarMaterial`, `DSTabBarCenterMaterial`). On iOS 26 and later both are Liquid Glass: the bar in its notch-less rounded rect, the control in the morph shape. On iOS 27 the glass is untinted. On iOS 26 it is tinted with `bgSurfaceMain` at 50%. Earlier systems use the variable blur. The bar and the control are separate surfaces, so each tunes its own intensity. Dark themes add the grey wash and the open fill. Light themes (read from `colorScheme`, which the app sets from the theme) use a stronger blur with neither.
- **Rim** (`DSTabBarCenterRim`). A hairline along the morph shape's edge.
- **Dim layer** (`DSTabBarDimLayer`, hosted by `TabBarDimView`). Dims the whole screen behind an opening center panel. It follows `centerProgress`.
- **Grabber** (`DSTabBarCenterGrabber`). A handle on the open panel. It shows with the content.

Every width, height and radius is read from one progress value. The shape is valid at any point of a drag.

**Gradient rule.** Gradient strokes on a resizing shape must be wrapped in a drawing group. SwiftUI rasterises gradient strokes on the CPU, so an unwrapped stroke costs a full frame on every morph step.

**States.** The center panel has two states, and both use the same panel:

- **Scan state.** The camera is live. The center button opens it on a tap.
- **Search state.** The search field has focus. The panel fills the room above the bar. The bar row fades out while the panel stays.

Scan and Search change the layout with the same open motion. A Scan ↔ Search change while open does not close the panel.

**Gestures.** A tap opens the Scan state. An upward swipe on the button opens the panel into the Search state. A downward drag or a tap closes it. A flick decides the end of a drag. Without a flick, the halfway point decides. Content drags from the open panel never open the Search state. The panel's content reads `centerPanelDrag` (`DSTabBarCenterDrag`, an `@Entry` environment key) and passes its pan through it, so the same settle applies.

**Settling.** The control reports open or closed to the chrome (`onSettled`) when its own settle completes. It never reports during a drag. A UIKit-driven move through `TabBarBarViewModel.setCenterPanelOpen` reports at the end of its animation. A drag past open stretches the panel. A drag past closed clamps at closed. Both commit only on release and settle. A cancelled drag settles to the nearer end. A grab during a settle starts from the on-screen progress.

**Content visibility.** During a drag, the content (the scanner) and the grabber appear only after the drag covers most of the travel. A settle open shows them at once. They ride the panel's top edge while the shape crops them. On a close, the shape moves first. The content and the grabber fade out with the close's final part. The Search state's results list fades in after a state change. Under Reduce Motion, the progress snaps and the content cross-fades. A close waits for that cross-fade before the shape snaps.

**Content lifetime.** The chrome builds the content the first time the control shows it. The control removes the content when it removes the panel.

**Panel height.** The panel declares its height rather than measuring the content. `ScanPanelViewController.declaredContentHeight()` returns a `TabBarCenterContentHeight`:

- `.fits(CGFloat)` for a fixed height. `ScanPanelViewLayout.declaredHeight` owns the layout constants.
- `.fill` while search has focus. The chrome caps the panel at `availablePanelHeight`, the room between the bar row and the bottom of the status strip.

The chrome takes the width from `DSTabBarStyle.rowWidth(inAvailableWidth:)` of its own view width, not from the hosting view. The hosting view is laid out after the chrome. The chrome recomputes the height on layout and whenever the content reports a change. Measuring a live camera would start a second capture session, so the height is never measured from it.

**Search focus.** The scan panel's search field is always present. Focusing it puts the panel in the Search state. `ScanPanelViewController.onSearchFocusChanged` reports focus. `MainTabBarViewController` passes it to `TabBarBottomChromeController.setBarHidden(_:)`. `DSTabBarContainer`'s `isBarHidden` fades the bar row out while the panel stays. The bar returns when focus ends. Closing or folding the panel also ends focus. A pan that begins on the search field stays with the field and does not move the panel.

**Swipe into Search.** This flow opens the Search state from a swipe on the center button.

1. `DSTabBarCenterControl` takes two optional swipe callbacks. `onSwipeOpenBegan` fires once per drag. It fires when a drag on the button that began short of fully open moves up past the tap slop. Content drags from the open panel never fire it. The package does not know what the callback means.
2. `TabBarBottomChromeController` forwards both callbacks to `TabBarBarViewModel`. A nil `onCenterPanelSwipeOpenBegan` keeps a swipe opening like a tap. `onSwipeOpenLanding` is always set.
3. `MainTabBarViewController` holds `isSearchSwipePending`. The scan panel controller may not exist yet when the swipe begins. The panel declares `.fill`, so the travel runs to the filled panel's top.
4. `ScanPanelViewController.setSearchPending(_:)` shows the Search state layout without focus and keeps the camera off. The bar keeps the Scan state look.
5. `onSwipeOpenLanding` fires when the release settles open, partway through the settle. It fires at once under Reduce Motion. The chrome commits `.content(.scan)` first. `currentKeyboardLift` then sees the open scan panel when the keyboard frame changes. The host then calls `focusSearch()`, which runs the normal focus path.
6. `setCenterPanelOpen(true)` returns early because the progress is already open. The release animation is not restarted.
7. A close or a settle to closed drops the swipe and calls `setSearchPending(false)`. The panel returns to the Scan state's layout. The chrome reuses the content controller until a committed close. A swipe that never landed open never commits one.

**Keyboard.** The lift lives in `barModel.keyboardLift` and moves with the panel in one transaction.

**Search field and list.** The scan panel's field (`DSSearchRowView`), and its scan button share the field material, `DSTabBarFieldMaterialView`. The field switches its icon position between the Scan state and the Search state (`setSearchFocused`). The results list is shared with the full-screen Search Contact screen. Colours are DesignSystem theme tokens that follow all five themes. `DSTabBarFieldMaterialView` and `DSSearchRowView` re-apply their resolved colours when `DSThemeTrait` changes. `DSTabBarStyle.Palette` keeps only the values with no plain token: the glass stroke, the selection fill, the grabber, the glass wash, the open fill and the dim wash. Search type uses DS typography tokens, so the bar has no type style of its own. `DSSearchAvatar` shows the contact's image, otherwise `DSLetterAvatar` at `.s40`. The shared `DSAvatar` stays untouched.

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

**Haptics.** Each component plays feedback next to the gesture that triggers it: the center control on open and close, a slot on every tap. The chrome plays a heavy impact on a user tuck, a medium impact on a user restore, and selection ticks while a drag moves the bar. Auto-hide plays no haptic.

**Accessibility.** The center control is a button whose label changes while the panel is open, and the escape action closes the panel. All labels are `LocalizedStringResource`.

**Tucked bar and VoiceOver.** While the bar is tucked, the Edge glow is its only accessibility element, and activating it restores the bar. Focus moves to the glow on tuck and back to the bar on restore. Auto-hide never runs while VoiceOver is on.

**Reduce Motion.** Read live, and observed for changes. Moves that are not a drag fade (`DockletReducedFade`). A drag still follows the finger. Only the icons, labels and center glyph fade, and the Edge glow follows that fade. Turning Reduce Motion on during a settle ends that settle at its destination. The first-hide nudge does not play. A move between `shown` and `hidden` is written without animation.

## Shelf

The Apps slot opens the shelf, which grows upward from the bar row. Its height is the content height, capped at the space between the bar row and the top safe area. Beyond the cap the grid scrolls.

- The top row holds a connection status icon and a settings icon. Neither is a tab. Connection status opens as a page sheet. Settings opens as a modal navigation stack. Both close the shelf first. `MainTabBarWireframe` builds that stack through the injected `TabFactoryProtocol`. Opening device details while the Settings stack is the topmost presented controller pushes onto it.
- The grid holds one cell per open product. Tapping a cell mounts that product and closes the shelf. A long press opens a context menu with Close. The cell's Close accessibility action does the same.

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

`TabBarChromePassthroughView` claims only the Grab zone and, while a panel is open, the dismiss area. Every other touch goes to the content behind the chrome.

The fold pan recognizer starts on any part of the bar row, the center button and the center hole included, for a drag that the Path accepts as sideways. It does not start while the bar is `hidden` or a panel is open. It refuses touches in the Grab zone.

A second pan takes only touches in the Grab zone, in any direction. The two pans never share a touch.

## Fold

The Docklet's motion has a shared core and a Path. `DockletMotion` is the core. It owns the hide progress, the settle, the drag samples, the release rules, the selection ticks, the haptics, the rubber band and the content fade target. `DockletPath` is an enum that holds the route. Its one case is the Drawbridge, and that case's state (`DockletDrawbridgeState`) holds the grip and the release residual. The core reads Path-specific behavior only through `DockletPath`'s members. Each Path keeps its measurements in its own type, such as `DockletDrawbridge`.

The three states are `shown`, `tucked` and `hidden`. The bar's position between shown and tucked is one value, the hide progress: 0 when shown, 1 when tucked. `DockletMotion` (`polkadot-app/Modules/MainTabBar/Chrome/DockletMotion.swift`) owns that value. It is a pure value type with no UIKit and no clock, so the caller passes in time.

- **Shown.** The bar sits at the bottom of the screen.
- **Tucked.** On the Drawbridge, the bar turns a quarter and slides out to the right, and the Edge glow stays on the right edge (`DockletDrawbridge`).
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

**Interactive pops.** The fold follows the transition. When it ends, the chrome commits the incoming screen's state, or rebuilds the staying screen's state if the pop was cancelled.

**Drag.** A horizontal pan that starts on the bar follows the finger live. The Path decides which pans it accepts (`acceptsPan`). A touch catches a moving bar where it is. A release that changes the rest records the user override.

**Held pose.** While a finger holds the bar, `DockletHold` (`DockletHeldPose.swift`) sets the pose.

**Release.** `DockletMotion` applies these rules to every Path. Velocity comes from the recent samples, and a final pause is ignored. The bar tucks when projected progress is at least 0.5. A drag that came back more than 30% of its furthest travel returns to the rest it started from, but only if the drag peaked above 0.05 progress.

**Bringing it back.** A drag from the Grab zone catches a tucked bar and aims it at the finger (`DockletAim`). Release uses the tuck rules, plus a fast flick inward from the Path's edge, which restores the bar however short the drag.

**Animation.** A release starts a spring (`DockletSpring`) with the release velocity, driven by a `CADisplayLink`. `TabBarFoldController` is the only writer of the chrome surface transform.

**Auto-hide.** A shown bar tucks itself without a drag, in two cases:

- The first touch outside the Docklet tucks it at once.
- Otherwise it tucks 3 s after the last touch or hardware key press.

The Docklet is the bar, the open panel and the Grab zone. The Edge glow strip does not count while the bar is shown. A touch on the Docklet restarts the 3 s countdown and cancels a pending outside-touch tuck. Further outside touches do not restart that tuck. The tuck is stored as the user override.

Blockers hold the tuck: an open panel, the keyboard, a drag or settle in progress, a bar that is not shown, any TipKit tip on screen, R1 or Docklet (`tipShown`), and VoiceOver (`voiceOver`). While a blocker holds, an outside touch starts no tuck. Clearing the last blocker restarts the 3 s countdown from that moment.

`DockletAutoHide` is the pure policy, and `DockletTouchObserver` feeds it the touches.

**Quiet hide.** The auto-hide tuck settles quietly, with no haptic. Only the content inside the glass fades (icons, labels and center glyph), because alpha on a visual effect view breaks the effect. The fade target is set once per transition, and it stays while the bar is tucked.

**Nudge.** The first tuck that lands in a launch, user or auto-hide, starts a nudge: the bar moves a short distance in from the Path's edge and back. It plays once per launch. A drag or a restore cancels it. It is off under Reduce Motion.

### Adding a Path

A new Path adds one case to `DockletPath` and its own state type, as `DockletDrawbridgeState` does, and implements every `DockletPath` member. Keep `popoverArrowDirection` (`TabBarBottomChromeController.swift`) and `TabBarEdgeGlowView` in step with its edge.

The shared code keeps the release rules, the settles, the haptics, Auto-hide, the Reduce Motion fade, and the nudge's distance and timing. The restore flick also measures inward from `edge`. Hidden is not a Path. It stays in `TabBarFoldController.hiddenTransform()`.

## Tips

Onboarding tips are TipKit tips. `TabBarTipController` is the only presenter, and it shows one popover at a time. The R1 tips point at the center button and the status strip. The Docklet tips point at the Edge glow. R1 tips need the bar shown and laid out, with no tab-root check. Docklet tips need the bar tucked. So the two groups are never eligible together.

`TabBarTips.steps` lists the steps in order. `TabBarTipOrderedSequence` shows the first step whose tip should display.

### Docklet tips

- `DockletTuckedTip` shows the first time a swipe tuck lands. It explains how to bring the Docklet back.
- `DockletAutoHiddenTip` shows the first time an auto-hide tuck lands. It explains why the bar left. When it shows, it invalidates `DockletTuckedTip`, so the tucked tip never shows after the auto-hide tip.

**Trigger.** A landed tuck shows the tip for its `DockletTuckKind`, user or auto-hide. The chrome clears the tip when the bar leaves `tucked` and when any pan begins.

**Presentation.** The popover points at the Edge glow from the Path's edge. The Grab zone still takes touches while the tip is up, so a tap elsewhere does not close a Docklet tip.

**Tap.** A tap on the body of the tip closes it with `.tipClosed`, as on an R1 tip, so it does not return. The bar stays tucked. The Docklet tips have no close button.

**Return clears the tip.** Any return of the bar from `tucked` closes a Docklet tip: a drag from the Grab zone, a VoiceOver activation, opening the Scan panel from a screen (`openPanelShowingDocklet`), or a screen whose state is not `tucked`.

**Outside tap.** An outside tap closes an R1 tip for good. A Docklet tip ignores it (see Presentation). Every tip stays a popover on compact width.

**Other closes.** A slot tap or a center panel opening invalidates the tip on screen (`retireForUserInteraction`). A panel opening or leaving the screen closes the popover without invalidating it. Such a tip stays eligible.

### R1 tips

The R1 tips keep their style: a tap on the body closes the tip, and there is no close button. A tap outside the tip also closes it. Their rules are described in `TabBarTipDefinitions.swift`.

### Shared rules

- **Once per device.** Both Docklet tips use `MaxDisplayCount(1)`. TipKit keeps the count and invalidations, so seen state lives in its datastore.
- **Blocks auto-hide.** Any tip on screen, R1 or Docklet, blocks Auto-hide.
- **Reset.** Debug Settings "Reset Tips" (`TESTNET_FEATURE` builds) resets the TipKit datastore on the next launch.

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
- `TabBarChromeSurfaceView` reads both safe-area insets from its superview, not from itself. The fold transforms the surface, and a transformed view's own insets change with the transform.

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

- **Size the center panel from the chrome's width.** The hosting view has zero width when the chrome first lays out. A height computed from it starts the scan panel at the wrong size. SwiftUI then routes the first taps on the embedded UIKit panel to its own host view, so the search field ignores them until the panel is reopened.
- **Two actions have no slot.** `TabBarAction.scan` and `TabBarAction.connectionStatus` have no bar slot. The center control and the shelf open them directly, so the branches for them in `handleAction` are not reached from a slot tap.
