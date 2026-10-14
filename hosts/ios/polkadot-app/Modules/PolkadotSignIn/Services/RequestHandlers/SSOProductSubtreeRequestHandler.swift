import Foundation
import KeyDerivation
import Products

/// Returns the `//product//{productId}` public key. Consent-free per RFC-0022:
/// the response carries no secret material, and product accounts become public
/// on-chain once used. Only `AutoSigning` requires consent.
final class SSOProductSubtreeRequestHandler: SSORequestHandling {
    private let accountManager: ProductsAccountManaging
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let logger: LoggerProtocol

    init(
        accountManager: ProductsAccountManaging,
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.accountManager = accountManager
        self.messageSender = messageSender
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .productSubtreeRequest = message.latestContent() else { return false }
        return true
    }

    func handle(
        message: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        guard case let .productSubtreeRequest(request) = message.latestContent() else {
            return
        }

        logger.info("Product subtree request received from \(host.name)")

        let result: PolkadotHostRemoteMessage.ProductSubtreeResult

        do {
            let publicKey = try accountManager.deriveProductSubtreePublicKey(for: request.productId)
            result = .success(.init(productPublicKey: publicKey))
        } catch {
            logger.error("Failed to derive product subtree key: \(error)")
            result = .failure(Self.failureMessage(for: error))
        }

        let responseMessage = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.productSubtreeResponse(
                requestMessageId: message.messageId,
                result: result
            ))
        )

        do {
            try await messageSender.postMessage(responseMessage, to: host)
        } catch {
            logger.error("Failed to send product subtree response: \(error)")
        }
    }
}

private extension SSOProductSubtreeRequestHandler {
    // Hosts get a stable message; the underlying error stays in local logs.
    static func failureMessage(for error: Error) -> String {
        if error is ProductDerivationPathError {
            "invalid product id"
        } else {
            "failed to derive product subtree key"
        }
    }
}
