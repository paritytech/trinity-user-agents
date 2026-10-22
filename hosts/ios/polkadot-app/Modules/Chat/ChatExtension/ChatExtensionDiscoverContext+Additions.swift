import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// The Chat Extension v2 additions of a product room (07-LATER §3.4): features, the call and the contact. The host
/// keeps each value that the product sets, also a feature that this build does not serve, so that a later build
/// applies it.
protocol ChatProductRoomAdditions {
    /// The features that the product declared for a room, or ``Chat/RoomFeatures/hostDefault``.
    func roomFeatures(for chatBot: ChatExtensionBotProtocol, roomId: String) async throws -> Chat.RoomFeatures

    /// Replaces the features of a room whole.
    func setRoomFeatures(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        features: Chat.RoomFeatures
    ) async throws

    /// Shows or changes the call of a room, or removes it with nil. The call stays in memory only.
    func setRoomCall(for chatBot: ChatExtensionBotProtocol, roomId: String, call: Chat.RoomCall?) async throws

    /// Sets the contact of a room, and keeps it across restarts.
    func setRoomContact(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        contact: Chat.RoomContact
    ) async throws
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

    func setRoomCall(for chatBot: ChatExtensionBotProtocol, roomId: String, call: Chat.RoomCall?) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        calls.set(call, for: chatId)
    }

    func setRoomContact(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        contact: Chat.RoomContact
    ) async throws {
        let chatId = try await requireRoom(of: chatBot, roomId: roomId)
        try await saveDeclaration(Chat.RoomContactUpdate(chatId: chatId, contact: contact))
    }
}
