import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// The Chat Extension v2 declarations of a product room (02-DESIGN §5.1). Each request replaces one
/// declaration whole and keeps it across restarts, so the chat list draws the room without the worker.
protocol ChatProductRoomDeclaring {
    /// What the product declared for a room, or `ChatRoomError.unknownRoom`.
    func roomDeclarations(for chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.RoomDeclarations

    /// Replaces the configuration of a room. A change of the message store discards the stored messages.
    func setRoomConfig(for chatBot: ChatExtensionBotProtocol, roomId: String, config: Chat.RoomConfig) async throws

    /// Replaces the footer of a room, and keeps the footer of the first version in step with it.
    func setRoomFooterConfig(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        footer: Chat.RoomFooterConfig
    ) async throws

    func setRoomToolbar(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        actions: [Chat.RoomToolbarAction]
    ) async throws

    func setRoomSuggestions(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        suggestions: [Chat.RoomSuggestion]
    ) async throws

    func setRoomCommands(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        commands: [Chat.RoomCommand]
    ) async throws

    /// Shows the activity line of a room, or removes it with nil. The line stays in memory only.
    func setRoomActivity(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        activity: Chat.RoomActivity?
    ) async throws
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
    /// the messages of the other store (02-DESIGN §13 item 8). Each call starts a new generation, also with an
    /// unchanged configuration (02-DESIGN §8.2, O9). An open room then reloads its policy and asks the product
    /// again for its participants and its menus, as on Android.
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

    func setRoomFooterConfig(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        footer: Chat.RoomFooterConfig
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomFooterConfigUpdate(chatId: chatId, footer: footer))

        // The policy and the chat screen of the first version read this footer.
        let legacyFooter: Chat.RoomFooter = footer == .empty ? .empty : .textInput
        try await setRoomFooter(for: chatBot, roomId: roomId, footer: legacyFooter)
    }

    func setRoomToolbar(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        actions: [Chat.RoomToolbarAction]
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomToolbarUpdate(chatId: chatId, actions: actions))
    }

    func setRoomSuggestions(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        suggestions: [Chat.RoomSuggestion]
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomSuggestionsUpdate(chatId: chatId, suggestions: suggestions))
    }

    func setRoomCommands(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        commands: [Chat.RoomCommand]
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomCommandsUpdate(chatId: chatId, commands: commands))
    }

    /// The line tells what happens now, so it stays in memory. A restart of the app removes it.
    func setRoomActivity(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        activity: Chat.RoomActivity?
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        activities.set(activity, for: chatId)
    }
}

private extension ChatExtensionDiscoverContext {
    /// Removes the messages of a room with their reactions, unread marks, meta and extension history. The room
    /// and its declarations stay.
    static func discardMessages(of chat: CDChat, chatId: Chat.Id, in context: NSManagedObjectContext) throws {
        for message in (chat.messages as? Set<CDChatMessage>) ?? [] {
            context.delete(message)
        }

        for unread in (chat.unreadMessages as? Set<CDChatUnreadMessage>) ?? [] {
            context.delete(unread)
        }

        // The meta names messages of the store that the room leaves.
        for meta in (chat.messageMeta as? Set<CDChatMessageMeta>) ?? [] {
            context.delete(meta)
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
