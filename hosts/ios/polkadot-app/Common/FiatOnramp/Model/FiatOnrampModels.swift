import Foundation

// MARK: - Fiat Purchase Limits

struct FiatOnRampSessionId: Equatable, Hashable, Codable {
    let value: String

    static func new() -> Self {
        .init(value: UUID().uuidString)
    }
}

struct FiatOnRampTransactionId: Equatable, Hashable, Codable {
    let value: String
}

typealias FiatOnrampFiatPurchaseLimitsResponse = [FiatOnrampFiatPurchaseLimit]

struct FiatOnrampFiatPurchaseLimit: Codable, Equatable {
    let currencyCode: String
    let defaultAmount: Decimal
    let minimumAmount: Decimal
    let maximumAmount: Decimal
    let serviceProviderDetails: [String: FiatOnrampFiatPurchaseLimitDetails]?
}

extension FiatOnrampFiatPurchaseLimit {
    func allowedProviders(for amount: Decimal) -> Set<String> {
        guard let providerDetails = serviceProviderDetails else {
            return []
        }

        let allowed = providerDetails.filter { _, details in
            amount >= details.minimumAmount && amount <= details.maximumAmount
        }

        return Set(allowed.keys)
    }
}

struct FiatOnrampFiatPurchaseLimitDetails: Codable, Equatable {
    let defaultAmount: Decimal?
    let minimumAmount: Decimal
    let maximumAmount: Decimal
}

// MARK: - Quotes

struct FiatOnrampQuoteRequest: Hashable {
    let serviceProviders: [String]
    let sourceAmount: Decimal
}

struct FiatOnrampQuoteSummary: Equatable, Codable {
    let providerId: String
    let fiatAmount: Decimal
    let fiatCurrencyCode: String
    let cryptoAmount: Decimal
    let cryptoCurrencyCode: String
}

// MARK: - Widget Session

enum FiatOnrampWidgetSessionType: String, Codable {
    case buy = "BUY"
}

struct FiatOnrampWidgetSessionRequest: Hashable {
    let serviceProvider: String
    let sourceAmount: Decimal
    let sessionType: FiatOnrampWidgetSessionType = .buy
    let sessionId: FiatOnRampSessionId

    var redirectUrl: URL {
        AppConfig.DeepLink.fiatOnramp(sessionId: sessionId.value)
    }
}

struct FiatOnrampWidgetSessionResponse: Equatable {
    let sessionId: FiatOnRampSessionId
    let widgetUrl: URL
}

// MARK: - Transactions

enum FiatOnrampFundingState: String, Codable {
    case waitingForDeposit
    case settledAwaitingSwap
    case swapInProgress
    case completed
    case failed
}

struct FiatOnrampFundingUpdate {
    let state: FiatOnrampFundingState
    let execution: DepositExecutionItem?
}

enum FiatOnrampTransactionStatus: String, Codable {
    case pending
    case settling
    case settled
    case failed
    case unknown
}

struct FiatOnrampTransactionSummary: Equatable {
    let transactionId: FiatOnRampTransactionId
    let sessionId: FiatOnRampSessionId
    let status: FiatOnrampTransactionStatus
}

struct FiatOnrampTransactionQuery: Hashable {
    let sessionIds: Set<FiatOnRampSessionId>
}

// MARK: - Service Providers

struct FiatOnrampProviderSummary: Codable, Equatable {
    let id: String
    let name: String
    let iconUrl: URL
}
