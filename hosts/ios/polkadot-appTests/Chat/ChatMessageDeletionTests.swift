import Foundation
import Keystore_iOS
import Operation_iOS
import Testing
@testable import polkadot_app

/// The user deletes a message of a product room: the row becomes the tombstone (02-DESIGN §8.2, P4.6).
struct ChatMessageDeletionTests {
    private let facade = UserDataStorageTestFacade()
    private let bot = ChatSampleExtension(logger: StubLogger())

    @Test func deletingTheLastMessageShowsThePreviousMessageInTheChatList() async throws {
        let (older, newest) = try await makeRoomWithTwoMessages()
        try await react(to: newest)

        try await delete(newest)

        let room = try await storedRoom()
        #expect(room?.message?.messageId == older.messageId)
        #expect(room?.unreadDisplayMessageCount == 1)
        #expect(room?.hasIncomingReaction == false)

        let tombstone = try await storedMessage(newest.messageId)
        #expect(tombstone?.content == .deleted)
        #expect(tombstone?.reactions.isEmpty == true)
    }

    @Test func deletingAnOlderMessageKeepsTheLastMessage() async throws {
        let (older, newest) = try await makeRoomWithTwoMessages()

        try await delete(older)

        #expect(try await storedRoom()?.message?.messageId == newest.messageId)
        #expect(try await storedMessage(older.messageId)?.content == .deleted)
    }

    @Test func deletingTheOnlyMessageLeavesNoLastMessage() async throws {
        _ = try await makeContext().createRoom(for: bot, roomId: "room", name: nil, icon: nil)
        let only = message("only")
        try await save([only])

        try await delete(only)

        #expect(try await storedRoom()?.message == nil)
    }

    @Test func deletingAMissingMessageFails() async throws {
        _ = try await makeContext().createRoom(for: bot, roomId: "room", name: nil, icon: nil)

        await #expect(throws: (any Error).self) {
            try await delete(message("missing"))
        }
    }
}

/// The room screen hands a confirmed delete to its engine.
struct ChatInteractorDeleteTests {
    @Test func aConfirmedDeleteGoesToTheEngine() async {
        let engine = RecordingChatEngine()
        let interactor = ChatInteractor(
            chatId: .chatExtension("game.dot", roomId: "room"),
            engine: engine,
            reactionRepository: ChatReactionRepository(
                repository: AnyDataProviderRepository(InMemoryDataProviderRepository<Chat.MessageReaction>())
            ),
            foregroundVisibilityReporter: nil,
            notificationsCleaner: StubPushNotificationsCleaner()
        )

        interactor.deleteMessage(messageId: "m1")

        var calls = engine.calls.makeAsyncIterator()
        #expect(await calls.next() == .deleteMessage("m1"))
    }
}

private extension ChatMessageDeletionTests {
    var chatId: Chat.Id {
        .chatExtension(bot.identifier, roomId: "room")
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

    func message(_ text: String) -> Chat.LocalMessage {
        Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: "room", content: .text(text))
    }

    /// Two unread product messages, saved one after the other so the second is the last message.
    func makeRoomWithTwoMessages() async throws -> (Chat.LocalMessage, Chat.LocalMessage) {
        _ = try await makeContext().createRoom(for: bot, roomId: "room", name: nil, icon: nil)
        let older = message("older")
        let newest = message("newest")
        try await save([older])
        try await save([newest])
        return (older, newest)
    }

    func save(_ messages: [Chat.LocalMessage]) async throws {
        let mapper = ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator())
        try await facade.createRepository(mapper: AnyCoreDataMapper(mapper))
            .saveOperation({ messages }, { [] })
            .asyncExecute()
    }

    /// An unread reaction of the product on the message.
    func react(to message: Chat.LocalMessage) async throws {
        let reaction = Chat.LocalMessage.newExtensionMessage(
            bot.identifier,
            roomId: "room",
            content: .reacted(.init(messageId: message.messageId, emoji: "👍"))
        )
        try await save([reaction])

        let row = Chat.MessageReaction(
            messageId: message.messageId,
            emoji: "👍",
            origin: .chatExtension(bot.identifier),
            chatId: chatId,
            timestamp: 1
        )
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatMessageReactionMapper()))
            .saveOperation({ [row] }, { [] })
            .asyncExecute()
    }

    func delete(_ message: Chat.LocalMessage) async throws {
        let deletion = Chat.MessageDeletion(messageId: message.messageId)
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatMessageDeletionMapper()))
            .saveOperation({ [deletion] }, { [] })
            .asyncExecute()
    }

    func storedRoom() async throws -> Chat.LocalModel? {
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    func storedMessage(_ messageId: String) async throws -> Chat.LocalMessage? {
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatMessageEntityMapper()))
            .fetchOperation(by: { messageId }, options: RepositoryFetchOptions())
            .asyncExecute()
    }
}
