#if TESTNET_FEATURE
    import UIKit

    final class AppFactoryResetWireframe {}

    extension AppFactoryResetWireframe: AppFactoryResetWireframeProtocol {
        func detachCurrentSession(from view: AppFactoryResetViewProtocol?) {
            // Dismissed first: a hierarchy replaced while it still presents a sheet is not released.
            view?.controller.presentingViewController?.dismiss(animated: false)
            sceneDelegate?.showResetPlaceholder()
        }

        func navigateToFreshStart() {
            sceneDelegate?.restartScene()
        }

        func dismiss(from view: AppFactoryResetViewProtocol?) {
            view?.controller.presentingViewController?.dismiss(animated: true)
        }
    }

    private extension AppFactoryResetWireframe {
        var sceneDelegate: SceneDelegate? {
            let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
            return scene?.delegate as? SceneDelegate
        }
    }
#endif
