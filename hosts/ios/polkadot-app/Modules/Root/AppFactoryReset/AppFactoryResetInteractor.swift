#if TESTNET_FEATURE
    import Foundation

    final class AppFactoryResetInteractor {
        weak var presenter: AppFactoryResetInteractorOutputProtocol?

        private let resetService: AppFactoryResetService

        init(resetService: AppFactoryResetService) {
            self.resetService = resetService
        }
    }

    extension AppFactoryResetInteractor: AppFactoryResetInteractorInputProtocol {
        /// Captures the presenter strongly: by now the session is detached and the task is its only owner.
        func performReset() {
            let presenter = presenter

            Task { [resetService] in
                await resetService.resetAllData()
                await presenter?.didCompleteReset()
            }
        }
    }
#endif
