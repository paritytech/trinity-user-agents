import AsyncExtensions
import Foundation
import Keystore_iOS
import Operation_iOS
import Testing
import UIKitExt
@testable import polkadot_app

@Suite("Chat extension room metadata")
struct ChatExtensionRoomMetadataTests {
    private let storageFacade = UserDataStorageTestFacade()
    private let bot = StubRoomBot()

    /// The host places Jollity before its worker runs, so the worker's own `createRoom` always
    /// finds the room. That call is the only time the product says what its room looks like.
    @Test
    func anExistingRoomTakesTheNameAndIconTheProductRegisters() async throws {
        let context = makeContext()
        try await savePlacedRoom(name: "Fallback")

        let status = try await context.createRoom(for: bot, roomId: roomId, name: "Jollity", icon: pngIcon)

        #expect(status == .exists)
        #expect(try await storedRoom()?.roomMetadata == Chat.RoomMetadata(
            chatRelativeId: roomId,
            name: "Jollity",
            icon: pngIcon
        ))
    }

    /// The refresh rewrites the room alone, so the chat keeps the messages it already holds.
    @Test
    func refreshingARoomKeepsItsLastMessage() async throws {
        let context = makeContext()
        _ = try await context.createRoom(for: bot, roomId: roomId, name: nil, icon: nil)
        let message = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: roomId, content: .text("hello"))
        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        _ = try await context.createRoom(for: bot, roomId: roomId, name: "Jollity", icon: pngIcon)

        #expect(try await storedRoom()?.message?.messageId == message.messageId)
    }

    /// The worker's own icon for its room wins over the one its manifest declares.
    @Test
    func aRoomDrawsTheInlineIconItsWorkerRegistered() {
        let chat = room(icon: pngIcon)

        #expect(chat.peerMetadata(using: StubRegistry(bot: bot)).icon == .image(pngData))
    }

    /// Drawing a remote room icon needs a fetch the chats list does not make.
    @Test
    func aRoomWithARemoteIconShowsTheManifestIcon() {
        let chat = room(icon: "https://example.com/icon.png")

        #expect(chat.peerMetadata(using: StubRegistry(bot: bot)).icon == .product(domain: bot.identifier))
    }
}

private let roomId = "jollity"
private let pngData = Data([0x89, 0x50, 0x4E, 0x47])
private let pngIcon = "data:image/png;base64," + pngData.base64EncodedString()

private extension ChatExtensionRoomMetadataTests {
    var chatId: Chat.Id {
        .chatExtension(bot.identifier, roomId: roomId)
    }

    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(storageFacade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper())))
    }

    /// Orders messages in a counter of its own, since the test host has no shared storage directory.
    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        let counter = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let mapper = ChatMessageEntityMapper(orderAllocator: FileChatMessageOrderAllocator(fileURL: counter))
        return AnyDataProviderRepository(storageFacade.createRepository(mapper: AnyCoreDataMapper(mapper)))
    }

    func makeContext() -> ChatExtensionDiscoverContext {
        ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: storageFacade,
            operationQueue: OperationQueue(),
            logger: Logger.shared
        )
    }

    func savePlacedRoom(name: String) async throws {
        let chat = Chat.LocalModel.newChatWithRoom(
            extensionId: bot.identifier,
            roomId: roomId,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: roomId, name: name, icon: nil)
        )
        try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()
    }

    func storedRoom() async throws -> Chat.LocalModel? {
        try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    func room(icon: String) -> Chat.LocalModel {
        .newChatWithRoom(
            extensionId: bot.identifier,
            roomId: roomId,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: roomId, name: "Jollity", icon: icon)
        )
    }
}

extension InMemorySettingsManager: @retroactive ChatExtensionBotSettings {}

private final class StubRoomBot: ChatExtensionBotProtocol {
    let identifier: ChatExtension.Id = "dim2.paseo"

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(
            name: "Jollity",
            contactSource: .chat,
            icon: .product(domain: identifier),
            input: .empty,
            moreActions: []
        )
    }

    func deliverAutomaticMessages(_: ChatExtensionDiscoverContextProtocol) {}

    func attach(presentationView _: ControllerBackedProtocol) {}

    func process(
        message _: Chat.LocalMessage,
        lastProcessingOutcome _: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        .processed
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {}
}

private struct StubRegistry: ChatExtensionsRegistering {
    let bot: ChatExtensionBotProtocol

    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> {
        AsyncStream { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func discover() {}

    func getExtensions(for _: Chat.Id) -> [ChatExtending] {
        [bot]
    }

    func getChatExtensionBot(for _: ChatExtension.Id) -> ChatExtensionBotProtocol? {
        bot
    }

    func getWidgetProviders() -> [ChatExtensionWidgetProvider] {
        []
    }
}
