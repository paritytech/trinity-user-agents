import Foundation
import Products

/// Resolves the funding product's entry pages from remote config: one destination for topping up,
/// one for withdrawing. A destination is either a dot-domain (`getcash.dot`) or a full URL whose
/// path names the page (`https://getcash.dot/offramp`), exactly as published.
protocol FundingDomainProviding: Sendable {
    func fundingPage() async throws -> ProductPage
    func offrampPage() async throws -> ProductPage
    /// Product labels of both entry pages, read synchronously from remote config.
    func fundingLabels() -> Set<String>
}

enum FundingDomainError: Error {
    case remoteConfigUnavailable
    case destinationNotConfigured
    case networkUnavailable(underlying: Error)
    case destinationNotOnNetwork(destination: String, tld: String)
}

final class FundingDomainProvider: FundingDomainProviding, @unchecked Sendable {
    private let hostProvider: ProductHostProviding
    private let remoteConfig: @Sendable () -> RemoteAppConfig?
    private let logger: LoggerProtocol

    init(
        hostProvider: ProductHostProviding,
        remoteConfig: @escaping @Sendable () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.hostProvider = hostProvider
        self.remoteConfig = remoteConfig
        self.logger = logger
    }

    func fundingPage() async throws -> ProductPage {
        try await page(for: requireRemoteConfig().fundingUrl)
    }

    func offrampPage() async throws -> ProductPage {
        try await page(for: requireRemoteConfig().offrampUrl)
    }

    func fundingLabels() -> Set<String> {
        let config = remoteConfig()
        let pages = [config?.fundingUrl, config?.offrampUrl].compactMap { destination in
            destination.flatMap { hostProvider.page(navigationDestination: $0) }
        }
        return Set(pages.map(\.host.name))
    }
}

private extension FundingDomainProvider {
    func requireRemoteConfig() throws -> RemoteAppConfig {
        guard let config = remoteConfig() else {
            logger.error("Funding page unresolved: remote config is not loaded")
            throw FundingDomainError.remoteConfigUnavailable
        }

        return config
    }

    /// Awaits the chain TLD through the host provider, then parses the destination into a page.
    func page(for destination: String?) async throws -> ProductPage {
        guard let destination, !destination.isEmpty else {
            logger.error("Funding page unresolved: destination is not configured")
            throw FundingDomainError.destinationNotConfigured
        }

        do {
            return try await hostProvider.resolvePage(destination: destination)
        } catch let ProductPageResolutionError.tldUnavailable(underlying) {
            logger.error("Funding page unresolved: network TLD is unavailable: \(underlying)")
            throw FundingDomainError.networkUnavailable(underlying: underlying)
        } catch let ProductPageResolutionError.destinationNotOnNetwork(destination, tld) {
            logger.error("Funding page unresolved: \(destination) is not a product of .\(tld)")
            throw FundingDomainError.destinationNotOnNetwork(destination: destination, tld: tld)
        }
    }
}
