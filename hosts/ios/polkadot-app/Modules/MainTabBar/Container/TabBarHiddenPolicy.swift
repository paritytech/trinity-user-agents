import UIKit

@MainActor
enum TabBarHiddenPolicy {
    static func deriveFoldState(
        in stack: [UIViewController],
        showing target: UIViewController
    ) -> TabBarFoldDerived {
        if chromeIsHidden(target) {
            return .hidden
        }

        let targetIndex = stack.firstIndex(of: target) ?? stack.index(before: stack.endIndex)

        guard stack.indices.contains(targetIndex) else {
            return .none
        }

        let pushed = stack[...targetIndex].dropFirst()

        if pushed.contains(where: \.hidesBottomBarWhenPushed) {
            return .folded
        }

        return .none
    }

    static func isTabRoot(in stack: [UIViewController], showing target: UIViewController) -> Bool {
        target === stack.first
    }

    static func stackAfterCancelledPop(
        stack: [UIViewController],
        staying: UIViewController
    ) -> [UIViewController] {
        stack.contains(staying) ? stack : stack + [staying]
    }
}

private extension TabBarHiddenPolicy {
    static func chromeIsHidden(_ controller: UIViewController) -> Bool {
        (controller as? TabBarChromeVisibilityCustomizing).map { !$0.tabBarChromeIsVisible } ?? false
    }
}
