import Foundation
import Keystore_iOS
import Operation_iOS
import UIKitExt
@testable import polkadot_app

/// The chat store of one product on an in-memory database, reached the way the rust chat bridge reaches it:
/// through a bound `ProductChatSurface` and a real `ChatExtensionDiscoverContext`.
final class ProductRoomFixture {
    let storageFacade = UserDataStorageTestFacade()
    let bot = ProductRoomBotStub()
    let context: ChatExtensionDiscoverContext
    let surface = ProductChatSurface()

    init() {
        context = ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: storageFacade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
        surface.bind(.init(bot: bot, context: context), owner: self)
    }

    var productId: String {
        bot.identifier
    }

    func chatId(_ roomId: String) -> Chat.Id {
        .chatExtension(bot.identifier, roomId: roomId)
    }

    func bridge() async -> RustChatExecutionBridge {
        let productId = productId
        return await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies(productId: productId) },
            chatMessaging: surface
        )
    }

    func storedChat(_ roomId: String) async throws -> Chat.LocalModel? {
        try await storageFacade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .fetchOperation(by: { self.chatId(roomId).rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    /// The messages of a room, in their order in the room.
    func messages(_ roomId: String) async throws -> [Chat.LocalMessage] {
        try await storageFacade.createRepository(
            filter: .localMessages(from: chatId(roomId)),
            sortDescriptors: [NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: true)],
            mapper: AnyCoreDataMapper(ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator()))
        )
        .fetchAllOperation(with: RepositoryFetchOptions())
        .asyncExecute()
    }

    /// Saves a message as the chat screen saves one.
    func save(_ message: Chat.LocalMessage) async throws {
        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator()))
        )
        .saveOperation({ [message] }, { [] })
        .asyncExecute()
    }

    /// A message of the user in a room, as the chat screen writes it.
    func userMessage(_ content: Chat.LocalMessage.Content, roomId: String, id: String = UUID().uuidString)
        -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: chatId(roomId),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: Date().toChatTimestamp(),
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}

/// A product bot that does nothing itself. The fixture drives the store directly.
final class ProductRoomBotStub: ChatExtensionBotProtocol {
    let identifier: ChatExtension.Id = "game.dot"

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(name: "Game", icon: .product(domain: identifier), input: .empty, moreActions: [])
    }

    func deliverAutomaticMessages(_: ChatExtensionDiscoverContextProtocol) {}

    func attach(presentationView _: ControllerBackedProtocol) {}

    func process(
        message _: Chat.LocalMessage,
        lastProcessingOutcome _: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        .skipped
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {}
}
