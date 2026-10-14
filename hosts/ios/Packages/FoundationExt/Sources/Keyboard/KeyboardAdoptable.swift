import UIKit
import Foundation_iOS

public protocol KeyboardAdoptable: AnyObject {
    var keyboardHandler: KeyboardHandler? { get set }

    func updateWhileKeyboardFrameChanging(_ frame: CGRect)
}

public extension KeyboardAdoptable {
    func setupKeyboardHandler() {
        guard keyboardHandler == nil else {
            return
        }

        keyboardHandler = KeyboardHandler(with: nil)
        keyboardHandler?.animateOnFrameChange = { [weak self] keyboardFrame in
            self?.updateWhileKeyboardFrameChanging(keyboardFrame)
        }
    }

    func clearKeyboardHandler() {
        keyboardHandler = nil
    }
}

public extension KeyboardAdoptable where Self: UIViewController, Self: ViewHolder,
    RootViewType: KeyboardAdoptableViewLayout {
    func updateWhileKeyboardFrameChanging(_ frame: CGRect) {
        let localKeyboardFrame = view.convert(frame, from: nil)
        let bottomInset = view.bounds.height - localKeyboardFrame.minY

        if bottomInset > 0 {
            rootView.adoptToVisibleKeyboard(bottomInset: bottomInset)
        } else {
            rootView.adoptToHiddenKeyboard()
        }

        rootView.layoutIfNeeded()
    }
}

// MARK: - KeyboardAdoptableViewLayout

public protocol KeyboardAdoptableViewLayout: UIView {
    func adoptToVisibleKeyboard(bottomInset: CGFloat)
    func adoptToHiddenKeyboard()
}

// MARK: - KeyboardViewAdoptable

public protocol KeyboardViewAdoptable: KeyboardAdoptable {
    var targetBottomConstraint: NSLayoutConstraint? { get }
    var currentKeyboardFrame: CGRect? { get set }
    var shouldApplyKeyboardFrame: Bool { get }

    func offsetFromKeyboardWithInset(_ bottomInset: CGFloat) -> CGFloat
}

private enum KeyboardViewAdoptableConstants {
    static var keyboardHandlerKey: String = "io.novasama.papp.keyboard.handler"
    static var keyboardFrameKey: String = "io.novasama.papp.keyboard.frame"
}

extension KeyboardViewAdoptable where Self: UIViewController {
    var keyboardHandler: KeyboardHandler? {
        get {
            withUnsafePointer(to: &KeyboardViewAdoptableConstants.keyboardHandlerKey) {
                objc_getAssociatedObject(
                    self,
                    $0
                ) as? KeyboardHandler
            }
        }

        set {
            withUnsafePointer(to: &KeyboardViewAdoptableConstants.keyboardHandlerKey) {
                objc_setAssociatedObject(
                    self,
                    $0,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN
                )
            }
        }
    }

    var currentKeyboardFrame: CGRect? {
        get {
            withUnsafePointer(to: &KeyboardViewAdoptableConstants.keyboardFrameKey) {
                objc_getAssociatedObject(
                    self,
                    $0
                ) as? CGRect
            }
        }

        set {
            withUnsafePointer(to: &KeyboardViewAdoptableConstants.keyboardFrameKey) {
                objc_setAssociatedObject(
                    self,
                    $0,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN
                )
            }
        }
    }

    var shouldApplyKeyboardFrame: Bool { true }

    func updateWhileKeyboardFrameChanging(_: CGRect) {}

    func setupKeyboardHandler() {
        guard keyboardHandler == nil else {
            return
        }

        let keyboardHandler = KeyboardHandler(with: nil)
        keyboardHandler.animateOnFrameChange = { [weak self] keyboardFrame in
            guard let strongSelf = self else {
                return
            }

            strongSelf.currentKeyboardFrame = keyboardFrame
            strongSelf.applyCurrentKeyboardFrame()
        }

        self.keyboardHandler = keyboardHandler
    }

    func applyCurrentKeyboardFrame() {
        guard let keyboardFrame = currentKeyboardFrame else {
            return
        }

        if let constraint = targetBottomConstraint {
            if shouldApplyKeyboardFrame {
                apply(keyboardFrame: keyboardFrame, to: constraint)

                view.layoutIfNeeded()
            }
        } else {
            updateWhileKeyboardFrameChanging(keyboardFrame)
        }
    }

    private func apply(keyboardFrame: CGRect, to constraint: NSLayoutConstraint) {
        let localKeyboardFrame = view.convert(keyboardFrame, from: nil)
        let bottomInset = view.bounds.height - localKeyboardFrame.minY

        constraint.constant = -(bottomInset + offsetFromKeyboardWithInset(bottomInset))
    }
}
