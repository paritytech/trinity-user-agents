import CoreData
import Foundation
import SubstrateSdk

final class SeenReceiptProcessor {
    private let storageFacade: StorageFacadeProtocol
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.logger = logger
    }
}

// MARK: - IncomingMessageProcessing

extension SeenReceiptProcessor: IncomingMessageProcessing {
    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact) {
        let seenContents = extractSeenReceipts(from: messages)

        guard !seenContents.isEmpty else {
            return
        }

        Task { [weak self] in
            do {
                try await self?.apply(seenContents, from: contact.accountId)
            } catch {
                self?.logger.warning("Failed to apply seen receipt: \(error)")
            }
        }
    }
}

// MARK: - Receive

extension SeenReceiptProcessor {
    func extractSeenReceipts(from messages: [Chat.RemoteMessage]) -> [ChatRemoteMessageContent.SeenContent] {
        messages.compactMap { message in
            guard case let .seen(seenContent) = message.versioned.ensureV1()?.content else {
                return nil
            }

            return seenContent
        }
    }
}

// MARK: - Apply

extension SeenReceiptProcessor {
    func apply(_ contents: [ChatRemoteMessageContent.SeenContent], from accountId: AccountId) async throws {
        let chatId = Chat.Id.person(accountId)

        try await storageFacade.databaseService.performWrite { context in
            guard let chat: CDChat = try context.first(for: .chat(for: chatId.rawRepresentation)) else {
                return
            }

            guard let resolvedOrder = try context.seenReceiptOrder(for: contents, in: chatId) else {
                return
            }

            chat.peerSeenOrder = max(chat.peerSeenOrder, resolvedOrder)
        }
    }
}

private extension NSManagedObjectContext {
    func seenReceiptOrder(
        for contents: [ChatRemoteMessageContent.SeenContent],
        in chatId: Chat.Id
    ) throws -> Int64? {
        let orders = try contents.compactMap { content in
            try messageOrder(of: content.messageId, in: chatId)
                ?? outgoingOrder(atOrBefore: content.timestamp, in: chatId)
        }
        return orders.max()
    }

    func messageOrder(of messageId: String, in chatId: Chat.Id) throws -> Int64? {
        let message: CDChatMessage? = try first(
            for: NSCompoundPredicate(andPredicateWithSubpredicates: [
                NSPredicate.localMessages(from: chatId),
                NSPredicate.messages(withIds: [messageId])
            ])
        )

        return message?.order
    }

    func outgoingOrder(atOrBefore timestamp: UInt64, in chatId: Chat.Id) throws -> Int64? {
        let request = CDChatMessage.fetchRequest()
        request.predicate = NSPredicate.postedOutgoingMessages(
            in: chatId,
            timestampAtOrBefore: Int64(bitPattern: timestamp)
        )
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false)]
        request.fetchLimit = 1

        return try fetch(request).first?.order
    }
}
