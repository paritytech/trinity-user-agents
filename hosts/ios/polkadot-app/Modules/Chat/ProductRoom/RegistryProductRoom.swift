import Foundation
import PolkadotUI
import TrUAPIHost

/// Asks the bot of the product that the extension registry holds now. The store can make a new bot for a product,
/// so the room screen looks the bot up for each call.
struct RegistryProductRoom: ProductRoomInteracting, @unchecked Sendable {
    let extensionId: ChatExtension.Id
    let registry: ChatExtensionsRegistering

    /// The bot of the product now, or nil while the registry has none.
    var bot: ProductBot? {
        registry.getChatExtensionBot(for: extensionId) as? ProductBot
    }

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

extension RegistryProductRoom: ProductRoomFooterRendering {
    func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        guard let bot else {
            return AsyncThrowingStream { $0.finish(throwing: UnservedProductRoom.NotServed()) }
        }
        return await bot.renderFooter(roomId: roomId)
    }

    func dispatchFooterAction(roomId: String, actionId: String, payload: String?) async {
        await bot?.dispatchFooterAction(roomId: roomId, actionId: actionId, payload: payload)
    }

    /// The image loader of the bot of the product, or a loader that has no image.
    var widgetImageResolver: WidgetImageResolver {
        bot?.widgetImageResolver ?? WidgetImageResolver { _ in nil }
    }
}

extension RegistryProductRoom: ProductChatQuerying, ChatRoomLifecycle {
    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        guard let bot else { throw UnservedProductRoom.NotServed() }
        return try await bot.messagesPage(request)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        guard let bot else { throw UnservedProductRoom.NotServed() }
        return try await bot.userAction(request)
    }

    /// A room whose product has no bot yet holds nothing.
    func open(roomId: String) async throws {
        try await bot?.open(roomId: roomId)
    }

    func close(roomId: String) async {
        await bot?.close(roomId: roomId)
    }
}
