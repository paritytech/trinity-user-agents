import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// Reads what a product room holds, for the decisions of its bot: the declarations of the room and the
/// newest message among some messages.
protocol ProductRoomReading: Sendable {
    /// The declarations of a stored room, or nil when no such room exists.
    func declarations(of chatId: Chat.Id) async -> Chat.RoomDeclarations?

    /// The newest of the messages in the order of the room, or nil when the room holds none of them.
    func newestMessageId(among messageIds: Set<Chat.MessageId>, in chatId: Chat.Id) async -> Chat.MessageId?
}

final class ProductRoomReader: ProductRoomReading, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func declarations(of chatId: Chat.Id) async -> Chat.RoomDeclarations? {
        try? await databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return chat.map { Chat.RoomDeclarations(entity: $0.roomMetadata) }
        }
    }

    func newestMessageId(among messageIds: Set<Chat.MessageId>, in chatId: Chat.Id) async -> Chat.MessageId? {
        guard !messageIds.isEmpty else { return nil }

        return try? await databaseService.performRead { context in
            let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
            request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                .localMessages(from: chatId),
                .messages(withIds: Array(messageIds))
            ])
            request.sortDescriptors = [
                NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
                NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
            ]
            request.fetchLimit = 1

            return try context.fetch(request).first?.messageId
        }
    }
}
