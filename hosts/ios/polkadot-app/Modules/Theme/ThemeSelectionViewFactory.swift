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
        return makeController(model: model)
    }

    @MainActor
    private static func makeController(model: ThemeSelectionViewModel) -> UIViewController {
        let controller = HiddableThemeHostingController(rootView: ThemeSelectionView(model: model))
        controller.view.backgroundColor = .bgSurfaceMain
        return controller
    }
}

private final class HiddableThemeHostingController: UIHostingController<ThemeSelectionView>, HiddableBarWhenPushed {}
