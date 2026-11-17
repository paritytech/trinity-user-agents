import CoreData
import Foundation
import Operation_iOS

/// Writes one chat message in a transaction of its own, reusing ``ChatMessageEntityMapper`` for the
/// row, so grouping, chat resolution and `touchParent` stay in one place.
protocol ChatMessageTransactionalStoring: Sendable {
    /// Returns once the message is durable.
    func save(_ message: Chat.LocalMessage) async throws
}

final class ChatMessageTransactionalStore: ChatMessageTransactionalStoring, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let mapper = ChatMessageEntityMapper()

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func save(_ message: Chat.LocalMessage) async throws {
        try await databaseService.performWrite { [mapper] context in
            let entity = try Self.entity(for: message, in: context)
            try mapper.populate(entity: entity, from: message, using: context)
        }
    }
}

private extension ChatMessageTransactionalStore {
    /// Fetch-or-insert by message id, the way the repository's save does.
    static func entity(
        for message: Chat.LocalMessage,
        in context: NSManagedObjectContext
    ) throws -> CDChatMessage {
        let existing: CDChatMessage? = try context.first(
            for: NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), message.messageId)
        )

        return try existing ?? context.insertNew(CDChatMessage.self)
    }
}
