import Foundation

protocol FiatOnrampServicing {
    func fetchFiatPurchaseLimits() async throws -> FiatOnrampFiatPurchaseLimitsResponse

    func fetchCryptoQuote(
        _ request: FiatOnrampQuoteRequest
    ) async throws -> [FiatOnrampQuoteSummary]

    func createWidgetSession(
        _ request: FiatOnrampWidgetSessionRequest
    ) async throws -> FiatOnrampWidgetSessionResponse

    func fetchServiceProviders() async throws -> [FiatOnrampProviderSummary]

    func fetchTransaction(id: FiatOnRampTransactionId) async throws -> FiatOnrampTransactionSummary?

    func fetchTransactions(
        _ request: FiatOnrampTransactionQuery
    ) async throws -> [FiatOnrampTransactionSummary]
}
