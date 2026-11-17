import Coinage
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import SDKLogger

protocol TransferSubmitting {
    /// The export the payment carried by message `messageId` is reserved under. Derived, so a retry
    /// after a crash finds the same export.
    func exportId(messageId: Chat.MessageId) throws -> CoinageOperationId

    /// `messageId` is the id the caller pre-generated for this transfer, the id of the chat message
    /// that carries the memo.
    ///
    /// `commit` runs right before this transport makes the keys durable anywhere, and a transport that
    /// never gets that far must never run it: a committed export is one the wallet no longer counts.
    func sendTransfer(
        _ memo: TransferMemo,
        to recipient: AccountId,
        messageId: Chat.MessageId,
        commit: @escaping @Sendable () async throws -> Void
    ) async throws
}

extension TransferSubmitting {
    func exportId(messageId: Chat.MessageId) throws -> CoinageOperationId {
        try .chatSend(messageId: messageId)
    }
}

final class ContactChatSubmitter: TransferSubmitting {
    private let chatContactsProvider: ContactsLocalStorageServicing
    private let createMessageFactory: LocalMessageCreatingOperationMaking
    private let messageStore: ChatMessageTransactionalStoring
    private let logger: SDKLoggerProtocol?

    init(
        chatContactsProvider: ContactsLocalStorageServicing,
        createMessageFactory: LocalMessageCreatingOperationMaking,
        messageStore: ChatMessageTransactionalStoring,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.chatContactsProvider = chatContactsProvider
        self.createMessageFactory = createMessageFactory
        self.messageStore = messageStore
        self.logger = logger
    }

    func sendTransfer(
        _ memo: TransferMemo,
        to recipient: AccountId,
        messageId: Chat.MessageId,
        commit: @escaping @Sendable () async throws -> Void
    ) async throws {
        let optContact = try await chatContactsProvider.getContact(by: recipient).asyncExecute()

        guard let contact = optContact.flatMap({ $0 }) else {
            logger?.error("No chat contact for \(recipient.toHex()) — the memo has nowhere to go")

            throw TransferSubmitterError.noChatContact
        }

        let message = createMessageFactory.transferMessage(
            to: Chat.Id.person(contact.accountId),
            memo: memo,
            messageId: messageId
        )

        // The message row is what carries the keys.
        try await commit()
        try await messageStore.save(message)
    }
}

enum TransferSubmitterError: Error {
    /// The recipient has no chat contact, so no message can carry the memo.
    case noChatContact
}

/// A transport for flows that carry the memo themselves, from the moment this returns.
struct NoChatSubmitter: TransferSubmitting {
    func sendTransfer(
        _: TransferMemo,
        to _: AccountId,
        messageId _: Chat.MessageId,
        commit: @escaping @Sendable () async throws -> Void
    ) async throws {
        try await commit()
    }
}
