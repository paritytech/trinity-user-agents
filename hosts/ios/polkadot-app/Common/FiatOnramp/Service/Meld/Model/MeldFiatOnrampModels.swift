import Foundation
import SubstrateSdk

extension MeldFiatOnrampSupport {
    struct FiatPurchaseLimitsRequest {
        let filters: ServiceProviderFilters
        let includeDetails: Bool
    }

    struct ServiceProvidersRequest {
        let filters: ServiceProviderFilters
    }

    struct QuoteRequest: Encodable {
        let countryCode: String
        let destinationCurrencyCode: String
        let serviceProviders: [String]
        let sourceAmount: Decimal
        let sourceCurrencyCode: String
        let walletAddress: String
    }

    struct WidgetSessionRequest: Encodable {
        let sessionData: WidgetSessionData
        let sessionType: String
        let externalCustomerId: String?
        let externalSessionId: String?
    }

    struct WidgetSessionData: Encodable {
        let countryCode: String
        let destinationCurrencyCode: String
        let serviceProvider: String
        let sourceAmount: String
        let sourceCurrencyCode: String
        let walletAddress: String
        let redirectUrl: String
    }

    struct QuoteResponse: Decodable {
        let quotes: [Quote]
        let message: String?
        let error: String?
        let timestamp: String?
    }

    struct Quote: Decodable {
        let fiatAmountWithoutFees: Decimal
        let sourceCurrencyCode: String
        let destinationAmount: Decimal
        let destinationCurrencyCode: String
        let customerScore: Decimal
        let serviceProvider: String
    }

    struct WidgetSessionResponse: Codable, Equatable {
        let id: String
        let externalSessionId: String?
        let externalCustomerId: String?
        let customerId: String?
        let widgetUrl: URL?
        let serviceProviderWidgetUrl: URL?
        let token: String?
    }

    typealias ServiceProvidersResponse = [ServiceProvider]

    struct ServiceProvider: Decodable {
        let serviceProvider: String
        let name: String
        let logos: ServiceProviderLogos
    }

    struct ServiceProviderLogos: Decodable {
        let darkShort: URL
    }

    struct TransactionResponse: Decodable {
        let transaction: Transaction?
    }

    struct TransactionListResponse: Decodable {
        let transactions: [Transaction]
    }

    struct Transaction: Decodable {
        let id: String
        let status: String
        let externalSessionId: String?
        let externalCustomerId: String?
    }

    enum TransactionStatusMapper {
        private static let pendingStatuses: Set<String> = [
            "PENDING_CREATED",
            "PENDING",
            "PROCESSING",
            "AUTHORIZED",
            "TWO_FA_REQUIRED",
            "TWO_FA_PROVIDED"
        ]

        private static let settlingStatuses: Set<String> = [
            "SETTLING"
        ]

        private static let settledStatuses: Set<String> = [
            "SETTLED",
            "COMPLETED",
            "SUCCESS",
            "SUCCEEDED"
        ]

        private static let failedStatuses: Set<String> = [
            "DECLINED",
            "CANCELLED",
            "FAILED",
            "ERROR",
            "VOIDED",
            "AUTHORIZATION_EXPIRED",
            "REFUNDED",
            "REJECTED",
            "EXPIRED"
        ]

        static func map(_ status: String) -> FiatOnrampTransactionStatus {
            if pendingStatuses.contains(status) {
                return .pending
            }

            if settlingStatuses.contains(status) {
                return .settling
            }

            if settledStatuses.contains(status) {
                return .settled
            }

            if failedStatuses.contains(status) {
                return .failed
            }

            return .unknown
        }
    }

    static func hashWalletAddress(_ walletAddress: String) -> String? {
        let data = Data(walletAddress.utf8)
        guard let hash = try? data.blake2b32() else {
            return nil
        }

        return hash.toHex()
    }

    enum ServiceError: Error {
        case unsupportedChainAsset
        case missingWalletAddress
        case missingExternalSessionId
        case missingWidgetURL
    }
}
