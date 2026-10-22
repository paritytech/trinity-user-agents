import Foundation
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

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        try await current.reactions(request)
    }

    func editHistory(_ request: ProductChatEditHistoryQueryRequest) async throws -> ProductChatEditHistoryQueryItem {
        try await current.editHistory(request)
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

extension RegistryProductRoom: ProductChatLinkTaking {
    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem {
        guard let bot else { throw UnservedProductRoom.NotServed() }
        return try await bot.openLink(request)
    }
}
