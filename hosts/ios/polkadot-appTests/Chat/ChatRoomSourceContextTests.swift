import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The engine picks the message source of a room.
struct ChatRoomSourceContextTests {
    private let facade = UserDataStorageTestFacade()

    @Test func aProductRoomReadsFromTheDatabase() async {
        let context = makeContext(chatId: .chatExtension("product.dot", roomId: "lobby"))

        #expect(await context.source() is CoreDataMessageSource)
    }

    @Test func aPersonChatReadsFromTheDatabase() async {
        let context = makeContext(chatId: .person(Data(repeating: 1, count: 32)))

        #expect(await context.source() is CoreDataMessageSource)
    }

    /// The whole chat is in the window of the database, so no end of it pages.
    @Test func theDatabaseWindowHoldsTheWholeChat() async {
        let source = await makeContext(chatId: .chatExtension("product.dot", roomId: "lobby")).source()

        #expect(await source.loadOlder() == .end)
        #expect(await source.loadNewer() == .end)
        #expect(await !source.jump(to: "m1"))
    }

    @Test func openAndCloseReachTheLifecycleOnce() async {
        let lifecycle = RecordingChatRoomLifecycle()
        let context = makeContext(chatId: .chatExtension("product.dot", roomId: "lobby"), lifecycle: lifecycle)

        await context.open()
        await context.open()
        await context.close()
        await context.close()

        #expect(lifecycle.calls == [.open("lobby"), .close("lobby")])
    }

    @Test func aRoomThatCannotOpenHasNothingToClose() async {
        let lifecycle = RecordingChatRoomLifecycle(failsToOpen: true)
        let context = makeContext(chatId: .chatExtension("product.dot", roomId: "lobby"), lifecycle: lifecycle)

        await context.open()
        await context.close()

        #expect(lifecycle.calls == [.open("lobby")])
    }

    @Test func aPersonChatHasNoLifecycle() async {
        let lifecycle = RecordingChatRoomLifecycle()
        let context = makeContext(chatId: .person(Data(repeating: 1, count: 32)), lifecycle: lifecycle)

        await context.open()
        await context.close()

        #expect(lifecycle.calls.isEmpty)
    }
}

private extension ChatRoomSourceContextTests {
    func makeContext(
        chatId: Chat.Id,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle()
    ) -> ChatRoomSourceContext {
        let messageRepository: AnyDataProviderRepository<Chat.LocalMessage> = AnyDataProviderRepository(
            facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )
        let coreData = CoreDataMessageSource(
            chatId: chatId,
            chatExtensionRegistry: SingleBotRegistry(bot: ChatSampleExtension(logger: StubLogger())),
            messageProviderFactory: ChatMessageDataProviderFactory(),
            historyRepository: AnyDataProviderRepository(facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatExtensionProcessingHistoryMapper())
            )),
            messageRepository: messageRepository,
            messageStatusUpdateRepository: AnyDataProviderRepository(facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageStatusUpdateMapper())
            )),
            attachmentsUpdateRepository: AnyDataProviderRepository(facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(AttachmentUploadingMapper())
            )),
            logger: StubLogger()
        )

        return ChatRoomSourceContext(
            chatId: chatId,
            coreDataSource: coreData,
            services: ProductRoomServices(querying: nil, lifecycle: lifecycle),
            logger: StubLogger()
        )
    }
}
