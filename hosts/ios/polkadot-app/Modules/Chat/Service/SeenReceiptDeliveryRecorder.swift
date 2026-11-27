import CoreData
import Foundation
import SubstrateSdk

protocol SeenReceiptDeliveryRecording {
    func recordDelivered(
        _ contents: [ChatRemoteMessageContent.SeenContent],
        to peerAccountId: AccountId
    ) async throws
}

final class SeenReceiptDeliveryRecorder {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }
}

extension SeenReceiptDeliveryRecorder: SeenReceiptDeliveryRecording {
    func recordDelivered(
        _ contents: [ChatRemoteMessageContent.SeenContent],
        to peerAccountId: AccountId
    ) async throws {
        let chatId = Chat.Id.person(peerAccountId)

        try await storageFacade.databaseService.performWrite { context in
            guard let chat: CDChat = try context.first(for: .chat(for: chatId.rawRepresentation)) else {
                return
            }

            guard let newest = try context.newestDeliveredMessage(for: contents, in: chatId),
                  (newest.order, newest.timestamp) > (chat.lastSentSeenOrder, chat.lastSentSeenTimestamp) else {
                return
            }

            chat.lastSentSeenOrder = newest.order
            chat.lastSentSeenTimestamp = newest.timestamp
        }
    }
}

private extension NSManagedObjectContext {
    func newestDeliveredMessage(
        for contents: [ChatRemoteMessageContent.SeenContent],
        in chatId: Chat.Id
    ) throws -> CDChatMessage? {
        let request = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            NSPredicate.localMessages(from: chatId),
            NSPredicate.messages(withIds: contents.map(\.messageId)),
            NSCompoundPredicate(orPredicateWithSubpredicates: [
                NSPredicate.byStatus(.incoming(.new)),
                NSPredicate.byStatus(.incoming(.seen))
            ])
        ])
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
        ]
        request.fetchLimit = 1

        return try fetch(request).first
    }
}
