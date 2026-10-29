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

    /// Re-derives the lift on the centre spring, so a panel commit moves the chrome with the centre control.
    func refreshKeyboardLift() {
        DSTabBarStyle.animateCenterChange(in: view) {
            self.applyKeyboardLift()
        }
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
        guard !foldController.isFolded else {
            foldController.setUserOverride(.shown, velocityX: 0)
            return
        }
        panelController.setPanel(nil)
    }

    /// The keyboard's own animation drives the lift, so the chrome moves in step with the keyboard.
    @objc func handleKeyboardFrameChange(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let frame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect else {
            return
        }
        keyboardFrame = frame
        let duration = userInfo[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double ?? 0
        let curve = userInfo[UIResponder.keyboardAnimationCurveUserInfoKey] as? Int
            ?? UIView.AnimationCurve.easeInOut.rawValue
        UIView.animate(
            withDuration: duration,
            delay: 0,
            options: UIView.AnimationOptions(rawValue: UInt(curve) << 16)
        ) {
            self.applyKeyboardLift()
        }
    }

    /// Moves the chrome to the lift the keyboard and scan panel call for, then re-declares the room above it.
    func applyKeyboardLift() {
        foldController.setKeyboardLift(currentKeyboardLift())
        refreshAvailablePanelHeight()
        refreshCenterContentHeight()
    }

    /// While the scan panel is open, the chrome moves up until the keyboard covers the centre button's foot by
    /// `keyboardButtonOverlap`, keeping the panel above it clear. Any other keyboard, or a closed panel, returns the
    /// chrome to rest.
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
        guard recognizer.state == .ended else {
            return
        }

        foldController.foldPanEnded(
            translationX: recognizer.translation(in: chromeSurface).x,
            velocityX: recognizer.velocity(in: chromeSurface).x
        )
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

    /// A fold drag starts on the bar row, outside the centre hole where the centre control sits.
    func canStartFoldDrag(at touch: UITouch) -> Bool {
        // The hosting view is taller than the row while the centre panel is open, and the row sits at its bottom.
        let bounds = barHost.view.bounds
        let row = CGRect(
            x: 0,
            y: bounds.height - DSTabBarStyle.height,
            width: bounds.width,
            height: DSTabBarStyle.height
        )
        let point = touch.location(in: barHost.view)
        guard row.contains(point) else {
            return false
        }
        let pointInRow = CGPoint(x: point.x, y: point.y - row.minY)
        return !DSTabBarStyle.isInCenterHole(pointInRow, inRowOfWidth: row.width)
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

    /// Only a drag that leads horizontally folds the bar, so a vertical scroll past it is left alone. A tab root
    /// cannot fold, and a hidden bar has nothing to fold.
    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard let pan = recognizer as? UIPanGestureRecognizer else {
            return true
        }
        guard !foldController.isTabRoot, foldController.state != .hidden else {
            return false
        }
        let translation = pan.translation(in: chromeSurface)
        return abs(translation.x) > abs(translation.y)
    }
}
