import Foundation
import Products
import SwiftyBeaver

final class BrowseInteractor: BrowseInteractorInputProtocol {
    private let hostProvider: ProductHostProviding
    private let logger: LoggerProtocol
    weak var presenter: BrowseInteractorOutputProtocol?

    init(
        hostProvider: ProductHostProviding,
        logger: LoggerProtocol
    ) {
        self.hostProvider = hostProvider
        self.logger = logger
    }

    func resolveBrowseHost() {
        Task { [weak self] in
            guard let self else { return }

            do {
                let host = try await hostProvider.resolveHost(label: AppConfig.DotNs.dotNsBrowse)
                await notify(host: host)
            } catch {
                logger.error("Failed to resolve browse host: \(error)")
                await notify(host: nil)
            }
        }
    }
}

// MARK: - Private

private extension BrowseInteractor {
    @MainActor
    func notify(host: ProductHost?) {
        if let host {
            presenter?.didResolve(host: host)
        } else {
            presenter?.didFailResolving()
        }
    }
}
