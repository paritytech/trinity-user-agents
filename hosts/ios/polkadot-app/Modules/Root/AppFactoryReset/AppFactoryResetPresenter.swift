#if TESTNET_FEATURE
    import Foundation

    @MainActor
    final class AppFactoryResetPresenter {
        weak var view: AppFactoryResetViewProtocol?

        let interactor: AppFactoryResetInteractorInputProtocol
        let wireframe: AppFactoryResetWireframeProtocol

        init(
            interactor: AppFactoryResetInteractorInputProtocol,
            wireframe: AppFactoryResetWireframeProtocol
        ) {
            self.interactor = interactor
            self.wireframe = wireframe
        }
    }

    extension AppFactoryResetPresenter: AppFactoryResetPresenterProtocol {
        func actionStartOver() {
            interactor.performReset()
        }

        func actionDismiss() {
            wireframe.dismiss(from: view)
        }
    }

    extension AppFactoryResetPresenter: AppFactoryResetInteractorOutputProtocol {
        func didFailReset(_ error: Error) {
            Logger.shared.error("Reset failed: \(error)")
        }

        func didCompleteReset() {
            wireframe.navigateToFreshStart()
        }
    }
#endif
