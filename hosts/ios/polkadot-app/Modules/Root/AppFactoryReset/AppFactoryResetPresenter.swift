#if TESTNET_FEATURE
    import Foundation

    @MainActor
    final class AppFactoryResetPresenter {
        weak var view: AppFactoryResetViewProtocol?

        let interactor: AppFactoryResetInteractorInputProtocol
        let wireframe: AppFactoryResetWireframeProtocol

        private var isResetting = false

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
            guard !isResetting else { return }
            isResetting = true

            wireframe.detachCurrentSession(from: view)
            interactor.performReset()
        }

        func actionDismiss() {
            guard !isResetting else { return }
            wireframe.dismiss(from: view)
        }
    }

    extension AppFactoryResetPresenter: AppFactoryResetInteractorOutputProtocol {
        func didCompleteReset() {
            wireframe.navigateToFreshStart()
        }
    }
#endif
