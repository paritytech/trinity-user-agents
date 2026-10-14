import UIKit
import PolkadotUI

/// Keeps the hosted content's focus layout, the panel fill and the keyboard anchor in step with the
/// text input's focus. The keyboard notification is the preferred trigger: when a software keyboard
/// shows or hides, everything animates inside the keyboard's own animator. The field's editing
/// notifications schedule a one-hop fallback for the hardware-keyboard case, where no keyboard
/// notification arrives; it applies the focus layout only if the keyboard handler has not already.
@MainActor
final class TabBarInputFocusController: NSObject {
    private unowned let surface: TabBarChromeSurfaceView
    private unowned let panelController: TabBarPanelController
    private let content: () -> TabBarKeyboardTrackingContent?

    private var appliedFocus = false
    private var keyboardAnimator: UIViewPropertyAnimator?

    init(
        surface: TabBarChromeSurfaceView,
        panelController: TabBarPanelController,
        content: @escaping () -> TabBarKeyboardTrackingContent?
    ) {
        self.surface = surface
        self.panelController = panelController
        self.content = content
        super.init()
        registerObservers()
    }
}

private extension TabBarInputFocusController {
    func registerObservers() {
        let center = NotificationCenter.default
        center.addObserver(
            self,
            selector: #selector(handleKeyboardWillShow(_:)),
            name: UIResponder.keyboardWillShowNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(handleKeyboardWillHide(_:)),
            name: UIResponder.keyboardWillHideNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(handleEditingChanged),
            name: UITextField.textDidBeginEditingNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(handleEditingChanged),
            name: UITextField.textDidEndEditingNotification,
            object: nil
        )
    }

    @objc
    func handleKeyboardWillShow(_ notification: NSNotification) {
        guard content()?.isKeyboardInputFocused == true else {
            return
        }

        animate(matching: notification) { [weak self] in
            self?.surface.setPanelTracksKeyboard(true)
            self?.applyFocus(true)
        }
    }

    /// A hardware keyboard posts a hide while the field stays focused, so the hide only drops the
    /// anchor. Whether the content is focused is read from the field one hop later, where a real
    /// resign has landed by then and the change joins this animator.
    @objc
    func handleKeyboardWillHide(_ notification: NSNotification) {
        animate(matching: notification) { [weak self] in
            self?.surface.setPanelTracksKeyboard(false)
        }
        scheduleFocusSync()
    }

    @objc
    func handleEditingChanged() {
        scheduleFocusSync()
    }

    /// One hop lets a keyboard notification posted in the same responder change take over first.
    func scheduleFocusSync() {
        DispatchQueue.main.async { [weak self] in
            self?.syncFocusIfNeeded()
        }
    }

    func syncFocusIfNeeded() {
        let focused = content()?.isKeyboardInputFocused ?? false
        guard focused != appliedFocus else {
            return
        }

        if let keyboardAnimator, keyboardAnimator.isRunning {
            keyboardAnimator.addAnimations { [weak self] in
                self?.applyFocus(focused)
            }
            return
        }

        let animator = UIViewPropertyAnimator(
            duration: DSTabBarTabsPanelView.openDuration,
            dampingRatio: DSTabBarTabsPanelView.openDampingRatio
        )
        animator.addAnimations { [weak self] in
            self?.applyFocus(focused)
        }
        animator.startAnimation()
    }

    /// Runs inside an animation block. The measurement happens there because `preferredHeight`
    /// settles the hosted view's pending layout; outside it the camera's new frame would commit
    /// unanimated.
    func applyFocus(_ focused: Bool) {
        surface.setContentFillsAvailableHeight(focused)
        content()?.setKeyboardInputFocused(focused)
        appliedFocus = focused
        panelController.refreshHeightAfterLayout()
        surface.layoutIfNeeded()
    }

    /// Mirrors the keyboard's duration and curve so the anchor, the focus layout and the container
    /// height move with the keys. Deliberately not registered with the panel controller.
    func animate(matching notification: NSNotification, _ animations: @escaping () -> Void) {
        let userInfo = notification.userInfo
        let duration = userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? TimeInterval ?? 0.3
        let curveRawValue = userInfo?[UIResponder.keyboardAnimationCurveUserInfoKey] as? Int ?? 0
        let curve = UIView.AnimationCurve(rawValue: curveRawValue) ?? .linear

        let animator = UIViewPropertyAnimator(
            duration: duration,
            timingParameters: UICubicTimingParameters(animationCurve: curve)
        )
        animator.addAnimations(animations)
        animator.addCompletion { [weak self] _ in
            self?.keyboardAnimator = nil
        }
        keyboardAnimator = animator
        animator.startAnimation()
    }
}
