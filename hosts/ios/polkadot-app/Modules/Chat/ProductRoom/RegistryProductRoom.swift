import Foundation
import TrUAPIHost

/// Asks the bot of the product that the extension registry holds now. The store can make a new bot for a product,
/// so the room screen looks the bot up for each call.
struct RegistryProductRoom: ProductRoomInteracting, @unchecked Sendable {
    let extensionId: ChatExtension.Id
    let registry: ChatExtensionsRegistering

    private var current: ProductRoomInteracting {
        (registry.getChatExtensionBot(for: extensionId) as? ProductRoomInteracting) ?? UnservedProductRoom()
    }

    func messageMenu(
        _ request: ProductChatMessageActionsQueryRequest
    ) async throws -> ProductChatMessageActionsQueryItem {
        try await current.messageMenu(request)
    }

    func participants(_ request: ProductChatParticipantsQueryRequest) async throws -> ProductChatParticipantsQueryItem {
        try await current.participants(request)
    }

    func participantMenu(
        _ request: ProductChatParticipantActionsQueryRequest
    ) async throws -> ProductChatParticipantActionsQueryItem {
        try await current.participantMenu(request)
    }

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        try await current.reactions(request)
    }

    func publish(_ event: ChatEvent, roomId: String) async throws {
        try await current.publish(event, roomId: roomId)
    }
}
