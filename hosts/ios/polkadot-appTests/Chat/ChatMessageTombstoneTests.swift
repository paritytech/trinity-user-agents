import Foundation
import Keystore_iOS
import Operation_iOS
import Testing
@testable import polkadot_app

/// The `read` status and the deleted tombstone of Chat Extension v2 (02-DESIGN §8.2).
struct ChatMessageTombstoneTests {
    private let facade = UserDataStorageTestFacade()
    private let bot = ChatSampleExtension(logger: StubLogger())

    /// Stored rows carry these values, so they never change.
    @Test func theNewKindsKeepTheirStoredValues() {
        #expect(Chat.LocalMessage.Content.ContentType.deleted.rawValue == 246)
        #expect(Chat.LocalMessage.Status.outgoing(.read).rawValue == 6)
        #expect(Chat.LocalMessage.Status(rawValue: 6) == .outgoing(.read))
    }

    @Test func readIsAheadOfDeliveredForAnOutgoingMessage() {
        #expect(Chat.LocalMessage.Status.OutgoingStatus.delivered < .read)
    }

    @Test func aTombstoneIsNeverTheChatListPreview() {
        #expect(Chat.LocalMessage.Content.ContentType.deleted.isExcludedFromChatList)
        #expect(!Chat.LocalMessage.Content.ContentType.deleted.isIndependentMessageInChat)
    }

    @Test func aReadMessageAndATombstoneAreStoredAndReadBack() async throws {
        let context = makeContext()
        _ = try await context.createRoom(for: bot, roomId: "room", name: nil, icon: nil)
        let read = Chat.LocalMessage
            .newMessage(to: chatId, content: .text("seen"))
            .replacingStatus(.outgoing(.read))
        let tombstone = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: "room", content: .deleted)

        try await messageRepository.saveOperation({ [read, tombstone] }, { [] }).asyncExecute()

        let stored = try await messageRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
        #expect(stored.first { $0.messageId == read.messageId }?.status == .outgoing(.read))
        #expect(stored.first { $0.messageId == tombstone.messageId }?.content == .deleted)
    }

    @Test func aTombstoneDoesNotBecomeTheLastMessageOfTheRoom() async throws {
        let context = makeContext()
        _ = try await context.createRoom(for: bot, roomId: "room", name: nil, icon: nil)
        let message = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: "room", content: .text("hi"))
        let tombstone = Chat.LocalMessage.newExtensionMessage(bot.identifier, roomId: "room", content: .deleted)

        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()
        try await messageRepository.saveOperation({ [tombstone] }, { [] }).asyncExecute()

        let room = try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
        #expect(room?.message?.messageId == message.messageId)
    }
}

private extension ChatMessageTombstoneTests {
    var chatId: Chat.Id {
        .chatExtension(bot.identifier, roomId: "room")
    }

    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        let mapper = ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator())
        return AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(mapper)))
    }

    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper())))
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
}
