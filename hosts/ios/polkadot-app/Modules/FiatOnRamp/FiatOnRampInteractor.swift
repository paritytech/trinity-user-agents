import Foundation
import SubstrateSdk

final class FiatOnRampInteractor {
    weak var presenter: FiatOnRampInteractorOutputProtocol?
    private let fiatOnrampService: FiatOnrampServicing
    private let logger: LoggerProtocol
    private var limitsTask: Task<Void, Never>?

    init(
        fiatOnrampService: FiatOnrampServicing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.fiatOnrampService = fiatOnrampService
        self.logger = logger
    }

    deinit {
        limitsTask?.cancel()
    }
}

extension FiatOnRampInteractor: FiatOnRampInteractorInputProtocol {
    func setup() {
        fetchPurchaseLimits()
    }
}

private extension FiatOnRampInteractor {
    func fetchPurchaseLimits() {
        limitsTask = Task { @MainActor [weak self] in
            guard let self else {
                return
            }

            do {
                let response = try await fiatOnrampService.fetchFiatPurchaseLimits()
                guard !Task.isCancelled else {
                    return
                }
                presenter?.didReceive(purchaseLimit: response.first)
            } catch {
                logger.error("Fiat on-ramp limits fetch failed: \(error)")
            }
        }
    }
}
