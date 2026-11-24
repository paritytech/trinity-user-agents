import SwiftUI
import UIKit

/// Decides whether a safe-area change moves a hosted SwiftUI screen with animation.
enum SafeAreaChangeAnimation {
    /// Whether a change from `old` to `new` animates.
    ///
    /// Only a change limited to the bottom edge animates, and only while the screen is on screen and Reduce Motion
    /// is off. A change that also moves another edge, such as a rotation, applies at once.
    static func animates(
        from old: EdgeInsets,
        to new: EdgeInsets,
        isOnScreen: Bool,
        reduceMotion: Bool
    ) -> Bool {
        guard isOnScreen, !reduceMotion, old.bottom != new.bottom else {
            return false
        }
        return old.top == new.top && old.leading == new.leading && old.trailing == new.trailing
    }
}

/// How a hosted screen takes a change of its bottom safe area.
public enum BottomSafeAreaChange {
    /// The screen moves with the change on the controller's animation.
    case animated
    /// The screen takes the new inset at once.
    case immediate
    /// The screen keeps the bottom inset it has until the controller is told to apply the current one.
    case held
}

@MainActor
@Observable
final class HostSafeAreaInsets {
    var value = EdgeInsets()
}

/// The root view of a `SafeAreaFollowingHostingController`: the screen's own view, inset by the safe area the
/// controller hands it.
public struct SafeAreaFollowingRoot<Content: View>: View {
    var content: Content
    private let insets: HostSafeAreaInsets

    init(content: Content, insets: HostSafeAreaInsets) {
        self.content = content
        self.insets = insets
    }

    public var body: some View {
        // iOS 18 still hands SwiftUI the container safe area after `safeAreaRegions` excludes it, which would
        // count the insets twice.
        content
            .safeAreaPadding(insets.value)
            .ignoresSafeArea(.container)
    }
}

/// A hosting controller whose SwiftUI screen moves with a bottom safe-area change that UIKit animates, such as the
/// Docklet's Clearance when the Docklet tucks or shows in place.
///
/// SwiftUI lays out on a new UIKit safe area in one frame, because the change reaches it without an animation. This
/// controller hides the container safe area from SwiftUI and hands SwiftUI the same insets itself, inside an animated
/// transaction when only the bottom edge changed. Keyboard avoidance is unchanged.
///
/// A screen inside the tab container with content pinned to the bottom subclasses it, reads its view through
/// `content` rather than `rootView`, and overrides `bottomSafeAreaChange` when its content must hold still.
open class SafeAreaFollowingHostingController<Content: View>: UIHostingController<SafeAreaFollowingRoot<Content>> {
    private let insets: HostSafeAreaInsets
    private let bottomChangeAnimation: Animation

    /// The screen's own view.
    public var content: Content {
        get { rootView.content }
        set { rootView.content = newValue }
    }

    /// How the screen takes a bottom safe-area change now.
    ///
    /// A screen that returns `held` calls `applySafeAreaInsets()` when the hold ends. The other edges update during
    /// a hold.
    open var bottomSafeAreaChange: BottomSafeAreaChange { .animated }

    /// Hosts `content`, moving it on `bottomChangeAnimation` when the bottom safe area changes.
    public init(content: Content, bottomChangeAnimation: Animation = DSTabBarStyle.centerEaseOut) {
        let insets = HostSafeAreaInsets()
        self.insets = insets
        self.bottomChangeAnimation = bottomChangeAnimation
        super.init(rootView: SafeAreaFollowingRoot(content: content, insets: insets))
        safeAreaRegions = .keyboard
    }

    @available(*, unavailable)
    @MainActor @preconcurrency public dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override open func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)
        applySafeAreaInsets(allowingAnimation: false)
    }

    override open func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        applySafeAreaInsets()
    }

    /// Hands SwiftUI the current safe area, in the way `bottomSafeAreaChange` says.
    ///
    /// The controller calls it for every safe-area change. A screen calls it when a hold ends.
    public func applySafeAreaInsets() {
        applySafeAreaInsets(allowingAnimation: true)
    }
}

private extension SafeAreaFollowingHostingController {
    var currentSafeAreaInsets: EdgeInsets {
        let isRightToLeft = view.effectiveUserInterfaceLayoutDirection == .rightToLeft
        return EdgeInsets(
            top: view.safeAreaInsets.top,
            leading: isRightToLeft ? view.safeAreaInsets.right : view.safeAreaInsets.left,
            bottom: view.safeAreaInsets.bottom,
            trailing: isRightToLeft ? view.safeAreaInsets.left : view.safeAreaInsets.right
        )
    }

    func applySafeAreaInsets(allowingAnimation: Bool) {
        let change = bottomSafeAreaChange
        var current = currentSafeAreaInsets
        if change == .held {
            current.bottom = insets.value.bottom
        }
        let animates = allowingAnimation
            && change == .animated
            && SafeAreaChangeAnimation.animates(
                from: insets.value,
                to: current,
                isOnScreen: view.window != nil,
                reduceMotion: UIAccessibility.isReduceMotionEnabled
            )

        withTransaction(Transaction(animation: animates ? bottomChangeAnimation : nil)) {
            insets.value = current
        }
    }
}
