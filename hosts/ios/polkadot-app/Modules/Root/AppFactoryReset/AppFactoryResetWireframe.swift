#if TESTNET_FEATURE
    import UIKit
    import TrUAPIHost

    final class AppFactoryResetWireframe {}

    extension AppFactoryResetWireframe: AppFactoryResetWireframeProtocol {
        func detachCurrentSession(from view: AppFactoryResetViewProtocol?) {
            // Dismissed first: a hierarchy replaced while it still presents a sheet is not released.
            view?.controller.presentingViewController?.dismiss(animated: false)
            sceneDelegate?.showResetPlaceholder()
        }

        func presentResetFailure(_ error: Error, retry: @escaping () -> Void) {
            let alert = UIAlertController(title: "App reset failed", message: nil, preferredStyle: .alert)
            if case let NativeRuntimeConfigError.RuntimeUnavailable(reason) = error {
                alert.message = "Restart the app before trying again.\n\n\(reason)"
                alert.addAction(UIAlertAction(title: "OK", style: .default))
            } else {
                alert.message = String(describing: error)
                alert.addAction(UIAlertAction(title: "Try again", style: .default) { _ in retry() })
            }
            sceneDelegate?.window?.rootViewController?.present(alert, animated: true)
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
