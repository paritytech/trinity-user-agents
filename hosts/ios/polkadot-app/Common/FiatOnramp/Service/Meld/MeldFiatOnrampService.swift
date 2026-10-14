import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

struct MeldFiatOnrampConfiguration {
    let baseUrl: URL
    let basicAuthToken: String
    let fiatCurrencyCode: String
    let countryCode: String
    let chainAssetId: ChainAssetId
}

extension MeldFiatOnrampConfiguration {
    // The Basic auth token is injected from env-vars.sh / CI secrets via
    // GeneratedSecrets. Do not commit the production value to source.
    static var prod: Self = .init(
        baseUrl: URL(string: "https://api.meld.io/")!,
        basicAuthToken: GeneratedSecrets.meldBasicAuthToken,
        fiatCurrencyCode: "USD",
        countryCode: Locale.autoupdatingCurrent.region?.identifier ?? "US",
        chainAssetId: AppConfig.Assets.fiatOnrampFundedAsset
    )
}

final class MeldFiatOnrampService: FiatOnrampServicing {
    private let configuration: MeldFiatOnrampConfiguration

    init(configuration: MeldFiatOnrampConfiguration) {
        self.configuration = configuration
    }

    func fetchFiatPurchaseLimits() async throws -> FiatOnrampFiatPurchaseLimitsResponse {
        guard let assetContext = resolveAssetContext(),
              let baseFilters = resolveBaseProviderFilters(for: assetContext) else {
            throw MeldFiatOnrampSupport.ServiceError.unsupportedChainAsset
        }

        let meldRequest = MeldFiatOnrampSupport.FiatPurchaseLimitsRequest(
            filters: baseFilters,
            includeDetails: true
        )

        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "service-providers/limits/fiat-currency-purchases",
            queryItems: MeldFiatOnrampSupport.QueryBuilder.limitsItems(for: meldRequest)
        )

        return try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<FiatOnrampFiatPurchaseLimitsResponse>()
        )
    }

    func fetchCryptoQuote(
        _ request: FiatOnrampQuoteRequest
    ) async throws -> [FiatOnrampQuoteSummary] {
        guard let assetContext = resolveAssetContext() else {
            throw MeldFiatOnrampSupport.ServiceError.unsupportedChainAsset
        }

        guard let walletAddress = resolveWalletAddress(for: assetContext.chainAsset) else {
            throw MeldFiatOnrampSupport.ServiceError.missingWalletAddress
        }

        let meldRequest = MeldFiatOnrampSupport.QuoteRequest(
            countryCode: configuration.countryCode,
            destinationCurrencyCode: assetContext.cryptoCurrency,
            serviceProviders: request.serviceProviders,
            sourceAmount: request.sourceAmount,
            sourceCurrencyCode: configuration.fiatCurrencyCode,
            walletAddress: walletAddress
        )

        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "payments/crypto/quote",
            queryItems: [],
            httpMethod: HttpMethod.post.rawValue,
            params: meldRequest
        )

        let response: MeldFiatOnrampSupport.QuoteResponse = try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<MeldFiatOnrampSupport.QuoteResponse>()
        )
        return mapQuotes(response.quotes)
    }

    func createWidgetSession(
        _ request: FiatOnrampWidgetSessionRequest
    ) async throws -> FiatOnrampWidgetSessionResponse {
        guard let assetContext = resolveAssetContext() else {
            throw MeldFiatOnrampSupport.ServiceError.unsupportedChainAsset
        }

        guard let walletAddress = resolveWalletAddress(for: assetContext.chainAsset) else {
            throw MeldFiatOnrampSupport.ServiceError.missingWalletAddress
        }

        let amountText = NSDecimalNumber(decimal: request.sourceAmount).stringValue
        let sessionData = MeldFiatOnrampSupport.WidgetSessionData(
            countryCode: configuration.countryCode,
            destinationCurrencyCode: assetContext.cryptoCurrency,
            serviceProvider: request.serviceProvider,
            sourceAmount: amountText,
            sourceCurrencyCode: configuration.fiatCurrencyCode,
            walletAddress: walletAddress,
            redirectUrl: request.redirectUrl.absoluteString
        )
        let meldRequest = MeldFiatOnrampSupport.WidgetSessionRequest(
            sessionData: sessionData,
            sessionType: request.sessionType.rawValue,
            externalCustomerId: MeldFiatOnrampSupport.hashWalletAddress(walletAddress),
            externalSessionId: request.sessionId.value
        )

        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "crypto/session/widget",
            queryItems: [],
            httpMethod: HttpMethod.post.rawValue,
            params: meldRequest
        )

        let response = try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<MeldFiatOnrampSupport.WidgetSessionResponse>()
        )

        return try mapWidgetSession(response)
    }

    func fetchServiceProviders() async throws -> [FiatOnrampProviderSummary] {
        guard let assetContext = resolveAssetContext(),
              let baseFilters = resolveBaseProviderFilters(for: assetContext) else {
            throw MeldFiatOnrampSupport.ServiceError.unsupportedChainAsset
        }

        let meldRequest = MeldFiatOnrampSupport.ServiceProvidersRequest(filters: baseFilters)

        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "service-providers",
            queryItems: MeldFiatOnrampSupport.QueryBuilder.serviceProvidersItems(for: meldRequest)
        )

        let response: MeldFiatOnrampSupport.ServiceProvidersResponse = try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<MeldFiatOnrampSupport.ServiceProvidersResponse>()
        )
        return response.map { mapProvider($0) }
    }

    func fetchTransaction(id: FiatOnRampTransactionId) async throws -> FiatOnrampTransactionSummary? {
        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "payments/transactions/\(id.value)",
            queryItems: []
        )

        let response: MeldFiatOnrampSupport.TransactionResponse = try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<MeldFiatOnrampSupport.TransactionResponse>()
        )
        guard let transaction = response.transaction else {
            return nil
        }

        return try mapTransaction(transaction)
    }

    func fetchTransactions(
        _ request: FiatOnrampTransactionQuery
    ) async throws -> [FiatOnrampTransactionSummary] {
        let endpoint = MeldFiatOnrampSupport.Endpoint(
            baseUrl: configuration.baseUrl,
            path: "payments/transactions",
            queryItems: MeldFiatOnrampSupport.QueryBuilder.transactionsItems(for: request)
        )

        let response: MeldFiatOnrampSupport.TransactionListResponse = try await performRequest(
            endpoint: endpoint,
            responseFactory: JsonResponseResultFactory<MeldFiatOnrampSupport.TransactionListResponse>()
        )
        return try response.transactions.map { try mapTransaction($0) }
    }

    private func performRequest<R: Decodable>(
        endpoint: MeldFiatOnrampSupport.Endpoint,
        responseFactory: BaseNetworkResultFactory<R>
    ) async throws -> R {
        let wrapper = createRequestWrapper(
            endpoint: endpoint,
            responseFactory: responseFactory
        )

        return try await wrapper.asyncExecute()
    }

    private func createRequestWrapper<R: Decodable>(
        endpoint: MeldFiatOnrampSupport.Endpoint,
        responseFactory: BaseNetworkResultFactory<R>
    ) -> CompoundOperationWrapper<R> {
        let requestFactory = BlockNetworkRequestFactory { [configuration] in
            var request = URLRequest(url: endpoint.url)
            request.httpMethod = endpoint.httpMethod
            request.setValue(
                HttpContentType.json.rawValue,
                forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
            )
            request.setValue(
                "Basic \(configuration.basicAuthToken)",
                forHTTPHeaderField: "Authorization"
            )

            if let params = endpoint.params {
                request.httpBody = try JSONEncoder().encode(params)
            }

            return request
        }

        let operation = NetworkOperation(
            requestFactory: requestFactory,
            resultFactory: AnyNetworkResultFactory(factory: responseFactory)
        )

        return CompoundOperationWrapper(targetOperation: operation)
    }

    private func resolveAssetContext() -> MeldFiatOnrampSupport.AssetContext? {
        MeldFiatOnrampConfiguration.resolveAssetContext(for: configuration.chainAssetId)
    }

    private func resolveWalletAddress(for chainAsset: ChainAsset) -> String? {
        MeldFiatOnrampConfiguration.resolveWalletAddress(for: chainAsset)
    }

    private func mapWidgetSession(_ widgetSession: MeldFiatOnrampSupport.WidgetSessionResponse) throws
        -> FiatOnrampWidgetSessionResponse {
        guard let sessionId = widgetSession.externalSessionId else {
            throw MeldFiatOnrampSupport.ServiceError.missingExternalSessionId
        }

        guard let widgetUrl = widgetSession.serviceProviderWidgetUrl ?? widgetSession.widgetUrl else {
            throw MeldFiatOnrampSupport.ServiceError.missingWidgetURL
        }

        return FiatOnrampWidgetSessionResponse(sessionId: .init(value: sessionId), widgetUrl: widgetUrl)
    }

    private func mapTransaction(
        _ transaction: MeldFiatOnrampSupport.Transaction
    ) throws -> FiatOnrampTransactionSummary {
        guard let sessionId = transaction.externalSessionId else {
            throw MeldFiatOnrampSupport.ServiceError.missingExternalSessionId
        }

        return FiatOnrampTransactionSummary(
            transactionId: .init(value: transaction.id),
            sessionId: .init(value: sessionId),
            status: MeldFiatOnrampSupport.TransactionStatusMapper.map(transaction.status)
        )
    }

    private func mapProvider(_ provider: MeldFiatOnrampSupport.ServiceProvider) -> FiatOnrampProviderSummary {
        FiatOnrampProviderSummary(
            id: provider.serviceProvider,
            name: provider.name,
            iconUrl: provider.logos.darkShort
        )
    }

    private func mapQuotes(_ quotes: [MeldFiatOnrampSupport.Quote]) -> [FiatOnrampQuoteSummary] {
        let bestByProvider = quotes.reduce(into: [String: MeldFiatOnrampSupport.Quote]()) { result, quote in
            let providerId = quote.serviceProvider
            if let existing = result[providerId] {
                if quote.customerScore > existing.customerScore {
                    result[providerId] = quote
                }
            } else {
                result[providerId] = quote
            }
        }

        return bestByProvider.values
            .sorted { $0.customerScore > $1.customerScore }
            .map { quote in
                FiatOnrampQuoteSummary(
                    providerId: quote.serviceProvider,
                    fiatAmount: quote.fiatAmountWithoutFees,
                    fiatCurrencyCode: quote.sourceCurrencyCode,
                    cryptoAmount: quote.destinationAmount,
                    cryptoCurrencyCode: quote.destinationCurrencyCode
                )
            }
    }

    private func resolveBaseProviderFilters(
        for assetContext: MeldFiatOnrampSupport.AssetContext
    ) -> MeldFiatOnrampSupport.ServiceProviderFilters? {
        MeldFiatOnrampSupport.ServiceProviderFilters.meldBase(
            countryCode: configuration.countryCode,
            fiatCurrencyCode: configuration.fiatCurrencyCode,
            cryptoChain: assetContext.cryptoChain,
            cryptoCurrency: assetContext.cryptoCurrency
        )
    }
}
