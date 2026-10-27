import CoreData
import Foundation
import Keystore_iOS
import Operation_iOS
import StructuredConcurrency
import Testing
@testable import polkadot_app

/// `delete_room` removes the room and everything that the host keeps for it.
struct ChatRoomDeletionTests {
    private let facade = UserDataStorageTestFacade()
    private let bot = ChatSampleExtension(logger: StubLogger())

    @Test func deletingARoomRemovesItsChatMessagesReactionsHistoryAndDeclarations() async throws {
        let context = makeContext()
        let message = try await fillRoom("lobby", in: context)

        try await context.deleteRoom(for: bot, roomId: "lobby")

        #expect(try await storedChat("lobby") == nil)
        #expect(try await count(CDChatMessage.self) == 0)
        #expect(try await count(CDMessageContent.self) == 0)
        #expect(try await count(CDChatMessageReaction.self) == 0)
        #expect(try await count(CDChatUnreadMessage.self) == 0)
        #expect(try await count(CDChatRoomMetadata.self) == 0)
        #expect(try await history(for: message.chatId).isEmpty)
    }

    @Test func theOtherRoomsOfTheProductStay() async throws {
        let context = makeContext()
        _ = try await fillRoom("lobby", in: context)
        let kept = try await fillRoom("other", in: context)

        try await context.deleteRoom(for: bot, roomId: "lobby")

        let other = try await storedChat("other")
        #expect(other?.message?.messageId == kept.messageId)
        #expect(other?.roomDeclarations?.config == config)
        #expect(try await history(for: kept.chatId).count == 1)
    }

    @Test func deletingAnUnknownRoomIsRefused() async throws {
        await #expect(throws: ChatRoomError.self) {
            try await makeContext().deleteRoom(for: bot, roomId: "missing")
        }
    }

    @Test func theSurfaceForwardsADeleteToTheContext() async throws {
        let context = makeContext()
        let surface = ProductChatSurface()
        surface.bind(.init(bot: bot, context: context), owner: bot)
        _ = try await context.createRoom(for: bot, roomId: "lobby", name: nil, icon: nil)

        try await surface.deleteRoom(roomId: "lobby")

        #expect(try await storedChat("lobby") == nil)
    }

    @Test func theRemoverReportsAChatThatDoesNotExist() async throws {
        let remover = ChatRoomRemover(databaseService: facade.databaseService)

        #expect(try await !remover.removeRoom(chatId("missing")))
    }
}

/// An open room screen goes away when its room goes away.
@MainActor
struct ChatInteractorRoomRemovalTests {
    @Test func theScreenOfARemovedProductRoomCloses() async {
        let chatId = Chat.Id.chatExtension("game.dot", roomId: "lobby")
        let output = RecordingChatInteractorOutput()
        let engine = RecordingChatEngine()
        engine.metadataUpdates = [metadata(chatId), nil]

        let interactor = makeInteractor(chatId: chatId, engine: engine, output: output)
        interactor.setup()

        #expect(await firstEvents(2, of: output) == [.metadata(chatId), .removedChat])
    }

    /// Only a room of a product closes when its chat goes; a chat without a room is not closed here.
    @Test func aPersonChatDoesNotCloseWhenItsChatIsMissing() async {
        let chatId = Chat.Id.person(Data(repeating: 1, count: 32))
        let output = RecordingChatInteractorOutput()
        let engine = RecordingChatEngine()
        engine.metadataUpdates = [metadata(chatId), nil, metadata(chatId)]

        let interactor = makeInteractor(chatId: chatId, engine: engine, output: output)
        interactor.setup()

        #expect(await firstEvents(2, of: output) == [.metadata(chatId), .metadata(chatId)])
    }
}

private let config = Chat.RoomConfig(productSource: false, reactionDetails: false, reportSeen: true)

private extension ChatRoomDeletionTests {
    func chatId(_ roomId: String) -> Chat.Id {
        .chatExtension(bot.identifier, roomId: roomId)
    }

    func makeContext() -> ChatExtensionDiscoverContext {
        ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
    }

    /// A room with messages, a reaction, an unread mark, a history entry and declarations.
    func fillRoom(
        _ roomId: String,
        in context: ChatExtensionDiscoverContext,
        icon: String? = nil
    ) async throws -> Chat.LocalMessage {
        _ = try await context.createRoom(for: bot, roomId: roomId, name: "Room", icon: icon)
        try await context.setRoomMetadata(for: bot, roomId: roomId, name: "Room")
        try await context.setRoomConfig(for: bot, roomId: roomId, config: config)
        try await context.setRoomPreview(for: bot, roomId: roomId, preview: Chat.RoomPreview(
            timestamp: nil,
            badge: .unreadCount(1),
            lastMessage: nil,
            unseenReaction: false
        ))

        let media = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: roomId, content: .text("first"))
        let message = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: roomId, content: .text("hi"))
        let messages = AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(
            ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator())
        )))
        try await messages.saveOperation({ [media, message] }, { [] }).asyncExecute()

        let reaction = Chat.MessageReaction(
            messageId: message.messageId,
            emoji: "👍",
            origin: .user,
            chatId: message.chatId,
            timestamp: 1
        )
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatMessageReactionMapper()))
            .saveOperation({ [reaction] }, { [] })
            .asyncExecute()

        let history = ChatExtension.ProcessingHistory(
            messageId: message.messageId,
            chatId: message.chatId.rawRepresentation,
            extensionId: bot.identifier
        )
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatExtensionProcessingHistoryMapper()))
            .saveOperation({ [history] }, { [] })
            .asyncExecute()

        return message
    }

    func storedChat(_ roomId: String) async throws -> Chat.LocalModel? {
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .fetchOperation(by: { chatId(roomId).rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    func history(for chatId: Chat.Id) async throws -> [ChatExtension.ProcessingHistory] {
        try await facade.createRepository(
            filter: .chatExtensionHistoryForChat(chatId),
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatExtensionProcessingHistoryMapper())
        )
        .fetchAllOperation(with: RepositoryFetchOptions())
        .asyncExecute()
    }

    func count<Entity: NSManagedObject>(_: Entity.Type) async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: NSFetchRequest<Entity>(entityName: String(describing: Entity.self)))
        }
    }
}

private extension ChatInteractorRoomRemovalTests {
    func metadata(_ chatId: Chat.Id) -> ChatMetadata {
        ChatMetadata(
            chatId: chatId,
            peerMetadata: Chat.PeerMetadata(name: "Room", icon: .bot, input: .empty, moreActions: []),
            state: .created
        )
    }

    func makeInteractor(
        chatId: Chat.Id,
        engine: RecordingChatEngine,
        output: RecordingChatInteractorOutput
    ) -> ChatInteractor {
        let interactor = ChatInteractor(
            chatId: chatId,
            engine: engine,
            reactionRepository: ChatReactionRepository(
                repository: AnyDataProviderRepository(InMemoryDataProviderRepository<Chat.MessageReaction>())
            ),
            usernameStorage: StubUsernameStorage(),
            foregroundVisibilityReporter: nil,
            notificationsCleaner: StubPushNotificationsCleaner()
        )
        interactor.presenter = output
        return interactor
    }

    func firstEvents(
        _ count: Int,
        of output: RecordingChatInteractorOutput
    ) async -> [RecordingChatInteractorOutput.Event] {
        var events: [RecordingChatInteractorOutput.Event] = []
        for await event in output.events where event != .listModel {
            events.append(event)
            if events.count == count { break }
        }
        return events
    }
}
