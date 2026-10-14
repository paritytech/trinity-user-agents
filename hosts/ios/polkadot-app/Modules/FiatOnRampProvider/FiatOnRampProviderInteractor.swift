import Foundation

final class FiatOnRampProviderInteractor {
    weak var presenter: FiatOnRampProviderInteractorOutputProtocol?
    private let fiatOnrampService: FiatOnrampServicing
    private let fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol
    private let amount: Decimal
    private let purchaseLimit: FiatOnrampFiatPurchaseLimit?
    private let logger: LoggerProtocol
    private var providersTask: Task<Void, Never>?
    private var quotesTask: Task<Void, Never>?
    private var widgetTask: Task<Void, Never>?
    private var cachedProviders: [FiatOnrampProviderSummary] = []

    init(
        fiatOnrampService: FiatOnrampServicing,
        fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol,
        amount: Decimal,
        purchaseLimit: FiatOnrampFiatPurchaseLimit?,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.fiatOnrampService = fiatOnrampService
        self.fiatOnrampTrackingService = fiatOnrampTrackingService
        self.amount = amount
        self.purchaseLimit = purchaseLimit
        self.logger = logger
    }

    deinit {
        providersTask?.cancel()
        quotesTask?.cancel()
        widgetTask?.cancel()
    }
}

extension FiatOnRampProviderInteractor: FiatOnRampProviderInteractorInputProtocol {
    func track(sessionId: FiatOnRampSessionId) {
        fiatOnrampTrackingService.startTracking(sessionId: sessionId)
    }

    func setup() {
        fetchProviders()
    }

    func select(providerId: String) {
        requestWidgetSession(for: providerId)
    }

    func refreshQuotes() {
        guard !cachedProviders.isEmpty else {
            return
        }

        fetchQuotes(for: cachedProviders)
    }
}

private extension FiatOnRampProviderInteractor {
    func fetchProviders() {
        providersTask?.cancel()
        providersTask = Task { [weak self] in
            guard let self else {
                return
            }

            do {
                let response = try await fiatOnrampService.fetchServiceProviders()
                guard !Task.isCancelled else {
                    return
                }
                let filtered = filterProviders(response)
                cachedProviders = filtered
                if filtered.isEmpty {
                    await presenter?.didReceive(providers: [], quotes: [])
                    return
                }
                fetchQuotes(for: filtered)
            } catch is CancellationError {
                return
            } catch {
                logger.error("Fiat on-ramp providers fetch failed: \(error)")
                await presenter?.didReceive(providers: [], quotes: [])
            }
        }
    }

    func filterProviders(
        _ providers: [FiatOnrampProviderSummary]
    ) -> [FiatOnrampProviderSummary] {
        guard let purchaseLimit else {
            return providers
        }

        guard let providerDetails = purchaseLimit.serviceProviderDetails,
              !providerDetails.isEmpty else {
            return providers
        }

        return providers.filter { provider in
            guard let details = providerDetails[provider.id] else {
                return true
            }

            return amount >= details.minimumAmount && amount <= details.maximumAmount
        }
    }

    func fetchQuotes(for providers: [FiatOnrampProviderSummary]) {
        quotesTask?.cancel()
        let providerIds = providers.map(\.id)

        let request = FiatOnrampQuoteRequest(
            serviceProviders: providerIds,
            sourceAmount: amount
        )

        quotesTask = Task { [weak self] in
            guard let self else {
                return
            }

            do {
                let quotes = try await fiatOnrampService.fetchCryptoQuote(request)
                guard !Task.isCancelled else {
                    return
                }
                await presenter?.didReceive(providers: providers, quotes: quotes)
            } catch is CancellationError {
                return
            } catch {
                logger.error("Fiat on-ramp quotes fetch failed: \(error)")
                await presenter?.didReceive(providers: providers, quotes: [])
            }
        }
    }

    func requestWidgetSession(for providerId: String) {
        widgetTask?.cancel()
        guard !providerId.isEmpty else {
            logger.warning("Fiat on-ramp provider id is empty.")
            Task {
                await presenter?.didFailWidgetSession()
            }
            return
        }

        let request = FiatOnrampWidgetSessionRequest(
            serviceProvider: providerId,
            sourceAmount: amount,
            sessionId: .new()
        )

        widgetTask = Task { [weak self] in
            guard let self else {
                return
            }

            do {
                let response = try await fiatOnrampService.createWidgetSession(request)
                guard !Task.isCancelled else {
                    return
                }
                await presenter?.didReceive(widgetSession: response)
            } catch is CancellationError {
                return
            } catch {
                logger.error("Fiat on-ramp widget session failed: \(error)")
                await presenter?.didFailWidgetSession()
            }
        }
    }
}
