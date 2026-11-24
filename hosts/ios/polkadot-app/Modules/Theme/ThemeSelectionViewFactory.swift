import DesignSystem
import PolkadotUI
import SwiftUI
import UIKit

enum ThemeSelectionViewFactory {
    @MainActor
    static func createView(observer: RootStateObserving) -> UIViewController {
        let model = ThemeSelectionViewModel(themeManager: ThemeManager.shared) { [observer] in
            observer.didSelectTheme()
        }
        let controller = UIHostingController(rootView: ThemeSelectionView(model: model))
        controller.view.backgroundColor = .bgSurfaceMain
        return controller
    }

    @MainActor
    static func createView(
        onComplete: @escaping () -> Void
    ) -> UIViewController {
        let model = ThemeSelectionViewModel(
            themeManager: ThemeManager.shared,
            context: .settings,
            onComplete: onComplete
        )
        let controller = ThemeSelectionHostingController(content: ThemeSelectionView(model: model))
        controller.view.backgroundColor = .bgSurfaceMain
        controller.onLeaving = { model.back() }
        return controller
    }
}

private final class ThemeSelectionHostingController: SafeAreaFollowingHostingController<ThemeSelectionView> {
    /// Runs as the screen starts leaving the stack, by the back button or a swipe, so the screen beneath is current
    /// before it slides in. A cancelled swipe also runs it.
    var onLeaving: (() -> Void)?

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        guard isMovingFromParent else {
            return
        }
        onLeaving?()
    }
}
