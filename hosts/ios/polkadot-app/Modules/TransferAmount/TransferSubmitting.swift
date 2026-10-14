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
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
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

        // The message row is what carries the keys, so its transaction is where the handoff becomes
        // final and the payment's transactions are registered.
        try await messageStore.save(message, onSaved: onSaved)
    }
}

enum TransferSubmitterError: Error {
    /// The recipient has no chat contact, so no message can carry the memo.
    case noChatContact
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
