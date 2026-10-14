import CryptoKit
import FoundationExt
import Foundation
import SDKLogger
import SubstrateSdk

final class W3sPayDeeplinkService {
    private let launcher: any W3sPayLaunching
    private let logger: SDKLoggerProtocol?

    init(launcher: any W3sPayLaunching, logger: SDKLoggerProtocol? = nil) {
        self.launcher = launcher
        self.logger = logger
    }
}

extension W3sPayDeeplinkService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard url.host() == Constants.host, url.path() == Constants.path else {
            return false
        }
        guard
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            let queryItems = components.queryItems,
            let id = queryItems.first(where: { $0.name == Constants.queryId })?.value,
            let amountString = queryItems.first(where: { $0.name == Constants.queryAmount })?.value,
            let keyString = queryItems.first(where: { $0.name == Constants.queryKey })?.value
        else {
            return false
        }

        guard
            isValidId(id),
            let amount = W3sAmount.parse(amountString, maxUnits: Constants.maxAmountUnits),
            let merchantKey = Data(base64URLEncoded: keyString),
            (try? Curve25519.KeyAgreement.PublicKey(rawRepresentation: merchantKey)) != nil,
            let topic = try? makeTopic(for: id)
        else {
            logger?.debug("W3sPayDeeplinkService: rejected malformed deeplink")
            return false
        }

        let name = queryItems.first(where: { $0.name == Constants.name })?.value?.removingPercentEncoding ?? id

        Task { @MainActor [launcher] in
            launcher.launch(
                merchantKey: merchantKey,
                topic: topic,
                paymentId: id,
                amount: amount,
                // No merchant config on the deeplink path — fall back to the id.
                recipientLabel: name
            )
        }
        return true
    }
}

private extension W3sPayDeeplinkService {
    enum Constants {
        static let host = "pay"
        static let path = "/cheque"
        static let queryId = "id"
        static let queryAmount = "amount"
        static let queryKey = "key"
        static let name = "name"
        static let maxAmountUnits: Decimal = 10_000
        static let topicPrefix = "pay-w3s:"
    }

    // ASCII-only — the merchant terminal hashes the id ASCII-strictly, so any
    // wider alphabet here would silently diverge from the merchant's topic.
    func isValidId(_ id: String) -> Bool {
        !id.isEmpty && id.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber) }
    }

    func makeTopic(for id: String) throws -> Data {
        let bytes = Data(Constants.topicPrefix.utf8) + Data(id.utf8)
        return try bytes.blake2b32()
    }
}
