import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatRoomRemoving: Sendable {
    /// Removes a product room and everything that the host keeps for it. Gives false when no such chat exists.
    func removeRoom(_ chatId: Chat.Id) async throws -> Bool
}

/// Removes a product room in one transaction (02-DESIGN §5.5). The cascade rules of the model remove the
/// messages, their reactions, the unread marks and the declarations with the chat. The extension
/// history of the room has no relationship to the chat, so the remover deletes it by the chat id.
final class ChatRoomRemover: ChatRoomRemoving, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(databaseService: CoreDataServiceProtocol) {
        self.databaseService = databaseService
    }

    func removeRoom(_ chatId: Chat.Id) async throws -> Bool {
        try await databaseService.performWrite { context in
            try Self.deleteRoom(chatId, in: context)
        }
    }
}

private extension ChatRoomRemover {
    static func deleteRoom(_ chatId: Chat.Id, in context: NSManagedObjectContext) throws -> Bool {
        let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))

        guard let chat else {
            return false
        }

        let historyRequest: NSFetchRequest<CDChatExtensionHistory> = CDChatExtensionHistory.fetchRequest()
        historyRequest.predicate = .chatExtensionHistoryForChat(chatId)
        historyRequest.includesPropertyValues = false

        for history in try context.fetch(historyRequest) {
            context.delete(history)
        }

        context.delete(chat)
        return true
    }
}
