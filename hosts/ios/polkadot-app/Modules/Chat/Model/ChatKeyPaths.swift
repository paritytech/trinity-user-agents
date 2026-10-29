import Foundation
import KeyDerivation
import SubstrateSdk

enum ChatKeyPathsError: Error, Equatable {
    case invalidJunction(String)
}

enum ChatKeyPaths {
    static func statement(tld: String, peerAccountId: AccountId) -> String {
        perChatPath(tld: tld, role: "peer-statement", peerAccountId: peerAccountId)
    }

    /// Shared by all chats: promotion only needs an unexpired authorization, so per-chat accounts just add claims.
    static func hop(tld: String) -> String {
        ProductDerivationPath.builtInRoot(BuiltInProduct.chat(for: tld)) + "//hop"
    }

    static func request(requestId: String, period: UInt32) throws -> String {
        try oneShotPath(namespace: "chat-request", requestId: requestId, period: period)
    }

    static func accept(requestId: String, period: UInt32) throws -> String {
        try oneShotPath(namespace: "chat-accept", requestId: requestId, period: period)
    }

    /// A request id becomes a path junction, so it can't be empty or contain the separator.
    static func validateRequestId(_ requestId: String) throws {
        guard !requestId.isEmpty, !requestId.contains("/") else {
            throw ChatKeyPathsError.invalidJunction(requestId)
        }
    }
}

private extension ChatKeyPaths {
    // The 0x prefix makes the junction parser take the peer id as raw chain code, not as a number or string.
    static func perChatPath(tld: String, role: String, peerAccountId: AccountId) -> String {
        ProductDerivationPath.builtInRoot(BuiltInProduct.chat(for: tld)) +
            "//\(role)//" + peerAccountId.toHex(includePrefix: true)
    }

    static func oneShotPath(namespace: String, requestId: String, period: UInt32) throws -> String {
        try validateRequestId(requestId)

        return "//\(namespace)//\(requestId)//\(period)"
    }
}
