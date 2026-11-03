import UIKit
import SwiftUI
import PolkadotUI
import SnapKit

// MARK: - Layout

extension TabBarBottomChromeController {
    /// Installed before the chrome surface so the dim sits below the bar and the open panels. It spans the whole
    /// screen, which the bar's host does not, and takes no touches so the chrome's passthrough hit test skips it.
    func installDimLayer() {
        addChild(dimHost)
        view.addSubview(dimHost.view)
        dimHost.view.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        dimHost.didMove(toParent: self)
        dimHost.view.backgroundColor = .clear
        dimHost.view.isUserInteractionEnabled = false
        dimHost.safeAreaRegions = []
    }

    func installChromeSurface() {
        chromeSurface.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(chromeSurface)
        chromeSurface.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    /// Sits above the chrome surface, so a tucked bar's glow is not moved by the bar's transform. It takes no touches,
    /// and the grab zone on the passthrough root does the restoring.
    func installEdgeGlow() {
        edgeGlowView.alpha = 0
        edgeGlowView.accessibilityLabel = String(localized: .Common.tabBarShowAccessibilityLabel)
        edgeGlowView.accessibilityTraits = .button
        edgeGlowView.onAccessibilityActivate = { [weak self] in
            self?.foldController.setUserOverride(.shown)
        }
        view.addSubview(edgeGlowView)
        let passthrough = view as? TabBarChromePassthroughView
        passthrough?.onGrabZoneTouchBegan = { [weak self] in
            self?.edgeGlowView.setHeld(true, animated: true)
        }
        passthrough?.onGrabZoneTouchEnded = { [weak self] in
            self?.edgeGlowView.setHeld(false, animated: true)
        }
    }

    func installOutsideTapRecognizer() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleOutsideTap))
        recognizer.delegate = self
        view.addGestureRecognizer(recognizer)
    }

    /// Any direction drags a tucked bar out from its Grab zone. The pan shares the fold pan's handler, which
    /// starts a grab because this pan sits on the root.
    func installGrabRecognizer() {
        let recognizer = UIPanGestureRecognizer(target: self, action: #selector(handleFoldPan(_:)))
        recognizer.delegate = self
        // Lets the grab touch reach the root, so the Edge glow stays held while the drag runs.
        recognizer.cancelsTouchesInView = false
        view.addGestureRecognizer(recognizer)
    }

    func installKeyboardObserver() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleKeyboardFrameChange(_:)),
            name: UIResponder.keyboardWillChangeFrameNotification,
            object: nil
        )
    }

    /// VoiceOver blocks the auto-hide while it runs. Reduce Motion is read live by each motion, so only its change is
    /// observed, to end a settle that is running.
    func installAccessibilityObservers() {
        handleVoiceOverStatusChange()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleVoiceOverStatusChange),
            name: UIAccessibility.voiceOverStatusDidChangeNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleReduceMotionStatusChange),
            name: UIAccessibility.reduceMotionStatusDidChangeNotification,
            object: nil
        )
    }

    /// A tucked bar is off screen, so VoiceOver reaches it through the Edge glow alone. Focus moves to the glow when
    /// the bar tucks, and to the bar when it comes back.
    func applyAccessibility(for state: TabBarVisibilityState) {
        let isTucked = state == .tucked
        chromeSurface.accessibilityElementsHidden = isTucked
        edgeGlowView.isAccessibilityElement = isTucked
        if isTucked {
            UIAccessibility.post(notification: .layoutChanged, argument: edgeGlowView)
        } else if state == .shown {
            UIAccessibility.post(notification: .layoutChanged, argument: chromeSurface)
        }
    }

    /// Reports each touch in the window to the auto-hide. The observer never recognizes, so no touch is taken.
    func installTouchObserver() {
        dockletTouchObserver.onTouchBegan = { [weak self] touch in
            self?.observeTouch(touch)
        }
    }

    /// Keeps the touch observer on the window that holds the chrome, which exists only once the chrome is laid out.
    func attachTouchObserver() {
        let window = view.window
        guard dockletTouchObserver.view !== window else {
            return
        }
        dockletTouchObserver.view?.removeGestureRecognizer(dockletTouchObserver)
        window?.addGestureRecognizer(dockletTouchObserver)
    }

    /// Re-derives the lift on the centre ease, so a panel commit moves the bar with the centre control.
    func refreshKeyboardLift() {
        applyKeyboardLift(animation: UIAccessibility.isReduceMotionEnabled ? nil : DSTabBarStyle.centerEaseOut)
    }

    func installBar() {
        addChild(barHost)
        chromeSurface.addBar(barHost.view)
        barHost.didMove(toParent: self)
        barHost.view.backgroundColor = .clear
        // The pinned bar geometry owns the home-indicator gap. The default safe area would apply it a second time.
        barHost.safeAreaRegions = []
        installCenterButtonAnchor()

        barModel.onSlotTapped = { [weak self] itemIndex in
            self?.handleSlotTap(itemIndex: itemIndex)
        }
        barModel.makeCenterContent = { [weak self] in
            guard let self else {
                return nil
            }
            if centerPanelContent == nil {
                centerPanelContent = makeCenterContent?()
            }
            return centerPanelContent
        }
        barModel.onCenterPanelOpenChanged = { [weak self] isOpen in
            self?.centerPanelOpenChanged(isOpen)
        }
        barModel.onCenterPanelCloseRequested = { [weak self] duration in
            self?.closeCenterPanel(duration: duration)
        }
        barModel.onShelfContentHeightChanged = { [weak self] in
            self?.applyBarLiveHeight()
        }
        barModel.onShelfChipTapped = { [weak self] id in
            self?.onChipTapped?(id)
        }
        barModel.onShelfChipCloseRequested = { [weak self] id in
            self?.onChipCloseRequested?(id)
        }
        barModel.onShelfSettingsTapped = { [weak self] in
            self?.onSettingsTapped?()
        }
        barModel.onShelfConnectionStatusTapped = { [weak self] in
            self?.onConnectionStatusTapped?()
        }

        let foldRecognizer = UIPanGestureRecognizer(target: self, action: #selector(handleFoldPan(_:)))
        foldRecognizer.delegate = self
        chromeSurface.addGestureRecognizer(foldRecognizer)

        // The resting centre button stands above the row, so its top has to be live before any layout pass.
        applyBarLiveHeight()
    }
}

private extension TabBarBottomChromeController {
    /// How far the keyboard covers the foot of the open centre panel's button.
    static let keyboardButtonOverlap: CGFloat = 4

    @objc func handleOutsideTap() {
        guard !foldController.isTucked else {
            foldController.setUserOverride(.shown)
            return
        }
        panelController.setPanel(nil)
    }

    @objc func handleVoiceOverStatusChange() {
        foldController.setAutoHideBlocked(.voiceOver, UIAccessibility.isVoiceOverRunning)
    }

    @objc func handleReduceMotionStatusChange() {
        foldController.reduceMotionChanged()
    }

    /// The keyboard's duration drives the lift. The lift, the room above the bar and the filled panel's height
    /// change in one SwiftUI transaction, so the panel's top edge stays still. A zero duration or Reduce Motion
    /// applies them at once.
    @objc func handleKeyboardFrameChange(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let frame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect else {
            return
        }
        keyboardFrame = frame
        let isKeyboardUp = view.convert(frame, from: nil).minY < view.bounds.maxY
        foldController.setAutoHideBlocked(.keyboardUp, isKeyboardUp)
        let duration = userInfo[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double ?? 0
        let animates = duration > 0 && !UIAccessibility.isReduceMotionEnabled
        applyKeyboardLift(animation: animates ? .spring(duration: duration, bounce: 0) : nil)
    }

    /// Moves the bar to the lift the keyboard and scan panel call for, then re-declares the room above it.
    func applyKeyboardLift(animation: Animation?) {
        withAnimation(animation) {
            barModel.keyboardLift = currentKeyboardLift()
            refreshAvailablePanelHeight()
            refreshCenterContentHeight(animation: animation)
        }
    }

    /// While the scan panel is open, the bar moves up until the keyboard covers the centre button's foot by
    /// `keyboardButtonOverlap`, keeping the panel above it clear. Any other keyboard, or a closed panel, returns the
    /// bar to rest.
    func currentKeyboardLift() -> CGFloat {
        guard panelController.open == .content(.scan), let keyboardFrame else {
            return 0
        }
        let keyboardTop = view.convert(keyboardFrame, from: nil).minY
        let keyboardOverlap = max(0, view.bounds.maxY - keyboardTop)
        let restingHeight = DSTabBarStyle.restingHeight(bottomSafeAreaInset: view.safeAreaInsets.bottom)
        let buttonFootOffset = restingHeight - DSTabBarStyle.centerButtonFootFromRowTop
        let allowedOverlap = buttonFootOffset + Self.keyboardButtonOverlap
        return max(0, keyboardOverlap - allowedOverlap)
    }

    @objc func handleFoldPan(_ recognizer: UIPanGestureRecognizer) {
        switch recognizer.state {
        case .began:
            tipController.dockletTuckEnded()
            let location = recognizer.location(in: view)
            if isGrabPan(recognizer) {
                foldController.grabPanBegan(location: location)
            } else {
                foldController.foldPanBegan(location: location)
            }
        case .changed:
            foldController.foldPanChanged(
                location: recognizer.location(in: view),
                translationX: recognizer.translation(in: view).x
            )
        case .ended,
             .cancelled:
            foldController.foldPanEnded()
        default:
            break
        }
    }

    /// The tip popover points at the resting centre button. Its frame follows from the bar geometry, because the
    /// button is drawn in SwiftUI and has no view of its own.
    func installCenterButtonAnchor() {
        barHost.view.addLayoutGuide(centerButtonAnchorGuide)
        let buttonSize = DSTabBarStyle.centerButtonRestingRise + DSTabBarStyle.centerButtonFootFromRowTop
        centerButtonAnchorGuide.snp.makeConstraints { make in
            make.centerX.equalTo(barHost.view)
            make.bottom.equalTo(barHost.view).offset(DSTabBarStyle.centerButtonFootFromRowTop - DSTabBarStyle.height)
            make.size.equalTo(buttonSize)
        }
    }

    /// A fold drag may start anywhere on the bar, centre button and centre hole included.
    func canStartFoldDrag(at touch: UITouch) -> Bool {
        touch.view?.isDescendant(of: barHost.view) ?? false
    }

    /// The pan on the root view, which drags a tucked bar out of its Grab zone. The other pan sits on the bar.
    func isGrabPan(_ recognizer: UIGestureRecognizer) -> Bool {
        recognizer.view === view
    }

    /// The tucked bar's Grab zone, which the fold controller publishes while tucked or settling toward tucked.
    func isInGrabZone(_ location: CGPoint) -> Bool {
        (view as? TabBarChromePassthroughView)?.foldGrabZone.contains(location) ?? false
    }

    /// A touch on the Docklet restarts its idle countdown and cancels a pending outside-touch tuck.
    func observeTouch(_ touch: UITouch) {
        let location = touch.location(in: view)
        foldController.recordTouch(isOnDocklet: isOnDocklet(at: location))
    }

    /// The bar's host, which holds the open panel, and the Grab zone count as the Docklet. The Edge glow does not,
    /// since it sits on the right edge even while the bar is shown.
    func isOnDocklet(at location: CGPoint) -> Bool {
        let barFrame = barHost.view.convert(barHost.view.bounds, to: view)
        return barFrame.contains(location) || isInGrabZone(location)
    }
}

// MARK: - UIGestureRecognizerDelegate

extension TabBarBottomChromeController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard recognizer is UIPanGestureRecognizer else {
            return touch.view === view
        }
        let touchIsInGrabZone = isInGrabZone(touch.location(in: view))
        if isGrabPan(recognizer) {
            return touchIsInGrabZone
        }
        return canStartFoldDrag(at: touch) && !touchIsInGrabZone
    }

    /// Only a drag that leads sideways folds the bar, so a vertical scroll past it is left alone. A hidden bar
    /// and an open panel take no drag.
    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard let pan = recognizer as? UIPanGestureRecognizer, !isGrabPan(pan) else {
            return true
        }
        guard foldController.state != .hidden, panelController.open == nil else {
            return false
        }
        let translation = pan.translation(in: view)
        return DockletDirectionLock.isSideways(
            translation: CGVector(dx: translation.x, dy: translation.y),
            isSettling: foldController.isSettling
        )
    }
}
