import Coinage
import CoreData
import DurableTransactions
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import SDKLogger

protocol TransferSubmitting {
    /// `messageId` is the id the caller pre-generated for this transfer, used both as the coinage
    /// transactions' groupId and as the id of the chat message that carries the memo.
    ///
    /// `onSaved` runs at this transport's own durability moment — the instant the keys are on their
    /// way — and, where the transport can, inside the very transaction that writes what carries them.
    /// A transport that never reaches that moment must never run it.
    func sendTransfer(
        _ memo: TransferMemo,
        to recipient: AccountId,
        messageId: Chat.MessageId,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws
}

enum TransferSubmitterError: Error {
    /// The recipient has no chat contact, so no message can carry the memo.
    case noChatContact
}

/// The room of the chat product that carries a payment.
struct ProductPaymentRoom: Equatable, Sendable {
    let productId: String
    let roomId: String
}

extension Chat.Id {
    /// The room of the chat product that a payment from its room screen goes to.
    var productPaymentRoom: ProductPaymentRoom? {
        guard case let .chatExtension(productId, roomId?) = self else { return nil }
        return ProductPaymentRoom(productId: productId, roomId: roomId)
    }
}

/// The transport while person chat runs in the chat product. The host keeps the payment with the room of the contact,
/// and the product sends it there (`SendPayment`) under the same message id.
struct ProductChatSubmitter: TransferSubmitting {
    /// The room of the chat that the payment starts from. Nil takes the room of the recipient among the rooms of the
    /// chat product, as the native chat looks up its contact.
    private let room: ProductPaymentRoom?
    private let contactRoom: @Sendable (AccountId) async throws -> ProductPaymentRoom?
    private let payments: ProductChatPayments
    private let now: @Sendable () -> UInt64
    private let logger: SDKLoggerProtocol?

    init(
        room: ProductPaymentRoom?,
        contactRoom: @escaping @Sendable (AccountId) async throws -> ProductPaymentRoom?,
        payments: ProductChatPayments,
        now: @escaping @Sendable () -> UInt64 = { Date().toChatTimestamp() },
        logger: SDKLoggerProtocol? = nil
    ) {
        self.room = room
        self.contactRoom = contactRoom
        self.payments = payments
        self.now = now
        self.logger = logger
    }

    func sendTransfer(
        _ memo: TransferMemo,
        to recipient: AccountId,
        messageId: Chat.MessageId,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        let contactRoom = if let room { room } else { try await contactRoom(recipient) }
        guard let room = contactRoom else {
            logger?.error("No chat product contact for \(recipient.toHex()): the memo has nowhere to go")

            throw TransferSubmitterError.noChatContact
        }

        // The payment row is what carries the keys, so its transaction is where the handoff becomes final and the
        // payment's transactions are registered.
        try await payments.send(
            memo,
            messageId: messageId,
            timestamp: now(),
            productId: room.productId,
            roomId: room.roomId,
            onSaved: onSaved
        )
    }
}

/// A transport for flows that carry the memo themselves. There is nothing to be atomic with, so the
/// hook runs in a transaction of its own.
struct NoChatSubmitter: TransferSubmitting {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        databaseService = storageFacade.databaseService
    }

    func sendTransfer(
        _: TransferMemo,
        to _: AccountId,
        messageId _: Chat.MessageId,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        try await databaseService.performWrite { context in
            try onSaved(CoreDataRegistrationScope(context: context))
        }
    }
}
