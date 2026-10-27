import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// The Chat Extension v2 declarations of a product room. Each request replaces one
/// declaration whole and keeps it across restarts, so the chat list draws the room without the worker.
protocol ChatProductRoomDeclaring {
    /// What the product declared for a room, or `ChatRoomError.unknownRoom`.
    func roomDeclarations(for chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.RoomDeclarations

    /// Replaces the configuration of a room. A change of the message store discards the stored messages.
    func setRoomConfig(for chatBot: ChatExtensionBotProtocol, roomId: String, config: Chat.RoomConfig) async throws
}

extension ChatExtensionDiscoverContext {
    func roomDeclarations(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String
    ) async throws -> Chat.RoomDeclarations {
        let chatId = Chat.Id.chatExtension(chatBot.identifier, roomId: roomId)

        let declarations = try await storageFacade.databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return chat.map { Chat.RoomDeclarations(entity: $0.roomMetadata) }
        }

        guard let declarations else {
            throw ChatRoomError.unknownRoom(roomId)
        }

        return declarations
    }

    /// Writes the configuration in one transaction with the discard of the messages, so a room never shows
    /// the messages of the other store. Each call starts a new generation, also with an unchanged configuration,
    /// as on Android. An open room then reloads its policy.
    func setRoomConfig(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        config: Chat.RoomConfig
    ) async throws {
        let chatId = Chat.Id.chatExtension(chatBot.identifier, roomId: roomId)

        try await storageFacade.databaseService.performWrite { context in
            guard let chat: CDChat = try context.first(for: .chat(for: chatId.rawRepresentation)) else {
                throw ChatRoomError.unknownRoom(roomId)
            }

            let stored = Chat.RoomDeclarations(entity: chat.roomMetadata).effectiveConfig
            if stored.productSource != config.productSource {
                try Self.discardMessages(of: chat, chatId: chatId, in: context)
            }

            let update = Chat.RoomConfigUpdate(chatId: chatId, config: config)
            try ChatRoomDeclarationUpdateMapper<Chat.RoomConfigUpdate>().populate(
                entity: chat,
                from: update,
                using: context
            )
        }
    }
}

extension ChatExtensionDiscoverContext {
    /// The chat of a room that the product created, or `ChatRoomError.unknownRoom`.
    func requireRoom(of chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.Id {
        let chatId = Chat.Id.chatExtension(chatBot.identifier, roomId: roomId)

        let existingChat = try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()

        guard existingChat != nil else {
            throw ChatRoomError.unknownRoom(roomId)
        }

        return chatId
    }

    func saveDeclaration<Update: ChatRoomDeclarationUpdating>(_ update: Update) async throws {
        let repository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Update>())
        )
        try await repository.saveOperation({ [update] }, { [] }).asyncExecute()
    }
}

enum ChatRoomError: LocalizedError {
    case unknownRoom(String)

    var errorDescription: String? {
        switch self {
        case let .unknownRoom(roomId):
            "no chat room \(roomId)"
        }
    }
}

private extension ChatExtensionDiscoverContext {
    /// Removes the messages of a room with their reactions, unread marks and extension history. The room
    /// and its declarations stay.
    static func discardMessages(of chat: CDChat, chatId: Chat.Id, in context: NSManagedObjectContext) throws {
        for message in (chat.messages as? Set<CDChatMessage>) ?? [] {
            context.delete(message)
        }

        for unread in (chat.unreadMessages as? Set<CDChatUnreadMessage>) ?? [] {
            context.delete(unread)
        }

        chat.lastDisplayMessage = nil

        let historyRequest: NSFetchRequest<CDChatExtensionHistory> = CDChatExtensionHistory.fetchRequest()
        historyRequest.predicate = .chatExtensionHistoryForChat(chatId)
        historyRequest.includesPropertyValues = false

        for history in try context.fetch(historyRequest) {
            context.delete(history)
        }
    }
}
