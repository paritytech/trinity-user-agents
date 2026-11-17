import Foundation
import SubstrateSdk

/// The 32-byte id of an operation the app starts in the core, in the app's own scope.
public struct CoinageOperationId: Hashable, Sendable {
    public let value: Data

    private init(value: Data) {
        self.value = value
    }

    /// A stable id for `label`, so that a retry joins the operation instead of starting a second one.
    public static func of(_ label: String) throws -> CoinageOperationId {
        try CoinageOperationId(value: Data(label.utf8).blake2b32())
    }

    /// A fresh id, for an operation nothing will ever need to join.
    public static func random() throws -> CoinageOperationId {
        try CoinageOperationId(value: Data.randomOrError(of: 32))
    }

    /// The export of the payment sent in chat message `messageId`.
    public static func chatSend(messageId: String) throws -> CoinageOperationId {
        try of("chat-send:\(messageId)")
    }

    /// The claim of the payment received in chat message `messageId`.
    public static func chatClaim(messageId: String) throws -> CoinageOperationId {
        try of("chat-claim:\(messageId)")
    }

    /// The reclaim of the payment sent in chat message `messageId` whose keys never left.
    public static func chatReclaim(messageId: String) throws -> CoinageOperationId {
        try of("chat-reclaim:\(messageId)")
    }
}
