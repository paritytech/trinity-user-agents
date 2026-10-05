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
        func performReset() {
            Task { @MainActor in
                do {
                    try await resetService.resetAllData()
                    presenter?.didCompleteReset()
                } catch {
                    presenter?.didFailReset(error)
                }
            }
        }
    }
#endif
