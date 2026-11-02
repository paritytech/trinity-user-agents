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
        edgeGlowView.backgroundColor = .bgSurfaceContainerInverted
        edgeGlowView.isUserInteractionEnabled = false
        edgeGlowView.alpha = 0
        view.addSubview(edgeGlowView)
    }

    func installOutsideTapRecognizer() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleOutsideTap))
        recognizer.delegate = self
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

    /// The keyboard's duration drives the lift. The lift, the room above the bar and the filled panel's height
    /// change in one SwiftUI transaction, so the panel's top edge stays still. A zero duration or Reduce Motion
    /// applies them at once.
    @objc func handleKeyboardFrameChange(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let frame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect else {
            return
        }
        keyboardFrame = frame
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
            foldController.foldPanBegan(touchX: recognizer.location(in: view).x)
        case .changed:
            foldController.foldPanChanged(translationX: recognizer.translation(in: view).x)
        case .ended:
            foldController.foldPanEnded(velocityX: recognizer.velocity(in: view).x)
        case .cancelled:
            foldController.foldPanEnded(velocityX: 0)
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
}

// MARK: - UIGestureRecognizerDelegate

extension TabBarBottomChromeController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard recognizer is UIPanGestureRecognizer else {
            return touch.view === view
        }
        return canStartFoldDrag(at: touch)
    }

    /// Only a drag that leads horizontally folds the bar, so a vertical scroll past it is left alone. A hidden bar
    /// and an open panel take no drag.
    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard let pan = recognizer as? UIPanGestureRecognizer else {
            return true
        }
        guard foldController.state != .hidden, panelController.open == nil else {
            return false
        }
        let translation = pan.translation(in: view)
        return abs(translation.x) > abs(translation.y)
    }
}
