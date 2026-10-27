import Foundation
import TrUAPIHost

/// Asks the bot of the product that the extension registry holds now. The store can make a new bot for a product,
/// so the room screen looks the bot up for each call.
struct RegistryProductRoom: @unchecked Sendable {
    /// The registry holds no bot of the product yet.
    struct MissingBot: Error {}

    let extensionId: ChatExtension.Id
    let registry: ChatExtensionsRegistering

    /// The bot of the product now, or nil while the registry has none.
    var bot: ProductBot? {
        registry.getChatExtensionBot(for: extensionId) as? ProductBot
    }
}

extension RegistryProductRoom: ProductChatQuerying, ChatRoomLifecycle {
    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        guard let bot else { throw MissingBot() }
        return try await bot.messagesPage(request)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        guard let bot else { throw MissingBot() }
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
