import Foundation

/// The feed updates that a product pushes into a room that the host stores.
protocol ChatProductFeedApplying {
    /// Applies one feed update. Throws `ChatRoomError.unknownRoom` or `Chat.ProductFeedError`.
    func applyFeedUpdate(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        update: Chat.ProductFeedUpdate
    ) async throws
}

extension ChatExtensionDiscoverContext: ChatProductFeedApplying {
    func applyFeedUpdate(
        for chatBot: ChatExtensionBotProtocol,
        roomId: String,
        update: Chat.ProductFeedUpdate
    ) async throws {
        let store = ChatProductFeedStore(
            databaseService: storageFacade.databaseService,
            orderAllocator: messageOrderAllocator
        )
        try await store.apply(update, productId: chatBot.identifier, roomId: roomId)
    }
}
