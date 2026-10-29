import DesignSystem
import SwiftUI
import UIKit

enum ThemeSelectionViewFactory {
    @MainActor
    static func createView(observer: RootStateObserving) -> UIViewController {
        let model = ThemeSelectionViewModel(themeManager: ThemeManager.shared) { [observer] in
            observer.didSelectTheme()
        }
        return makeController(model: model)
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
        let controller = makeController(model: model)
        controller.onLeaving = { model.back() }
        return controller
    }

    @MainActor
    private static func makeController(model: ThemeSelectionViewModel) -> ThemeSelectionHostingController {
        let controller = ThemeSelectionHostingController(rootView: ThemeSelectionView(model: model))
        controller.view.backgroundColor = .bgSurfaceMain
        return controller
    }
}

private final class ThemeSelectionHostingController: UIHostingController<ThemeSelectionView> {
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
