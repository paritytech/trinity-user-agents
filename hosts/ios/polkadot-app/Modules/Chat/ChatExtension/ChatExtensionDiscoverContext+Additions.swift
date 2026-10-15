import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// The Chat Extension v2 additions of a product room (07-LATER §3.4): features, message meta, the call, the user
/// state and the personas. The host keeps each value that the product sets, also a feature that this build does not
/// serve, so that a later build applies it.
protocol ChatProductRoomAdditions {
    /// The features that the product declared for a room, or ``Chat/RoomFeatures/hostDefault``.
    func roomFeatures(for chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.RoomFeatures

    /// Replaces the features of a room whole.
    func setRoomFeatures(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        features: Chat.RoomFeatures
    ) async throws

    /// Replaces the meta of one message. Throws `Chat.ProductFeedError.unknownMessage` for a message that a room
    /// of the host store does not hold.
    func setMessageMeta(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        messageId: String,
        meta: Chat.MessageMeta
    ) async throws

    /// Shows or changes the call of a room, or removes it with nil. The call stays in memory only.
    func setRoomCall(for chatBot: ChatExtensionBotProtocol, roomId: String, call: Chat.RoomCall?) async throws

    /// The rooms of the product that the user muted or archived.
    func roomUserStates(for chatBot: ChatExtensionBotProtocol) async throws -> [Chat.RoomUserStateEntry]

    /// The bot ids of the personas that the product registered, in alphabetical order.
    func personaIds(for chatBot: ChatExtensionBotProtocol) async throws -> [String]
}

extension ChatExtensionDiscoverContext: ChatProductRoomAdditions {
    func roomFeatures(for chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.RoomFeatures {
        try await roomDeclarations(for: chatBot, roomId: roomId).features
    }

    func setRoomFeatures(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        features: Chat.RoomFeatures
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomFeaturesUpdate(chatId: chatId, features: features))
    }

    func setMessageMeta(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        messageId: String,
        meta: Chat.MessageMeta
    ) async throws {
        let chatId = Chat.Id.chatExtension(chatBot.identifier, roomId: roomId)
        let store = ChatMessageMetaStore(databaseService: storageFacade.databaseService)
        try await store.setMeta(meta, productMessageId: messageId, chatId: chatId)
    }

    func setRoomCall(for chatBot: ChatExtensionBotProtocol, roomId: String, call: Chat.RoomCall?) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        calls.set(call, for: chatId)
    }

    func roomUserStates(for chatBot: ChatExtensionBotProtocol) async throws -> [Chat.RoomUserStateEntry] {
        let predicate = NSPredicate.roomChatsForExtension(chatBot.identifier)

        return try await storageFacade.databaseService.performRead { context in
            let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
            request.predicate = predicate

            return try context.fetch(request)
                .compactMap { chat -> Chat.RoomUserStateEntry? in
                    guard let metadata = chat.roomMetadata, let roomId = metadata.chatRelativeId else {
                        return nil
                    }
                    let state = Chat.RoomUserState(entity: metadata)
                    return state.isDefault ? nil : Chat.RoomUserStateEntry(roomId: roomId, state: state)
                }
                .sorted { $0.roomId < $1.roomId }
        }
    }

    func personaIds(for chatBot: ChatExtensionBotProtocol) async throws -> [String] {
        let productId = chatBot.identifier

        return try await storageFacade.databaseService.performRead { context in
            let request: NSFetchRequest<CDProductChatPersona> = CDProductChatPersona.fetchRequest()
            request.predicate = .personas(of: productId)

            return try context.fetch(request).compactMap(\.botId).sorted()
        }
    }
}

extension NSPredicate {
    static func personas(of productId: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDProductChatPersona.productId), productId)
    }
}
