import Foundation
import Operation_iOS
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The engine picks the message source of a room from its policy.
struct ChatRoomSourceContextTests {
    private let facade = UserDataStorageTestFacade()
    private let chatId = Chat.Id.chatExtension("product.dot", roomId: "lobby")

    @Test func aProductSourceRoomReadsFromTheProduct() async {
        let context = makeContext(policy: policy(store: .product))

        #expect(await context.source() is ProductMessageSource)
        #expect(await context.messageStore() == .product)
    }

    @Test func aHostStoreRoomReadsFromTheDatabase() async {
        let context = makeContext(policy: policy(store: .host))

        #expect(await context.source() is CoreDataMessageSource)
        #expect(await context.messageStore() == .host)
    }

    @Test func aPersonChatReadsFromTheDatabase() async {
        let context = makeContext(chatId: .person(Data(repeating: 1, count: 32)), policy: policy(store: .product))

        #expect(await context.source() is CoreDataMessageSource)
    }

    /// The whole chat is in the window of the database, so no end of it pages.
    @Test func theDatabaseWindowHoldsTheWholeChat() async {
        let source = await makeContext(policy: policy(store: .host)).source()

        #expect(await source.loadOlder() == .end)
        #expect(await source.loadNewer() == .end)
        #expect(await !source.jump(to: "m1"))
    }

    @Test func theProductSourceListensToTheFeedOfItsRoom() async throws {
        let listeners = ProductFeedListeners()
        let context = makeContext(policy: policy(store: .product), feedListeners: listeners)
        let source = try #require(await context.source() as? ProductMessageSource)
        await source.start()
        await source.waitForFirstPage()

        let other = Chat.Id.chatExtension("product.dot", roomId: "other")
        #expect(await listeners.deliver(.new(message("m1")), roomId: "lobby", to: chatId))
        #expect(await !listeners.deliver(.new(message("m2")), roomId: "other", to: other))

        #expect(await source.snapshot.messages.map(\.messageId) == ["m1"])
    }

    /// The product sets the configuration of its room after the room screen picked the source: the screen then
    /// reads from the product.
    @Test func aConfigurationAfterTheFirstPickChangesTheSource() async {
        let store = OSAllocatedUnfairLock(initialState: ChatRoomPolicy.MessageStore.host)
        let context = makeContext(policy: { [self] in policy(store: store.withLock { $0 }) })

        #expect(await context.source() is CoreDataMessageSource)
        #expect(await !context.refresh())

        store.withLock { $0 = .product }
        #expect(await context.refresh())
        #expect(await context.source() is ProductMessageSource)
        #expect(await !context.refresh())
    }

    @Test func openAndCloseReachTheLifecycleOnce() async {
        let lifecycle = RecordingChatRoomLifecycle()
        let context = makeContext(policy: policy(store: .host), lifecycle: lifecycle)

        await context.open()
        await context.open()
        await context.close()
        await context.close()

        #expect(lifecycle.calls == [.open("lobby"), .close("lobby")])
    }

    @Test func aRoomThatCannotOpenIsUnavailable() async throws {
        let lifecycle = RecordingChatRoomLifecycle(failsToOpen: true)
        let context = makeContext(policy: policy(store: .product), lifecycle: lifecycle)
        let source = try #require(await context.source() as? ProductMessageSource)
        await source.start()
        await source.waitForFirstPage()

        await context.open()

        #expect(await source.snapshot.readiness == .unavailable)
        await context.close()
        #expect(lifecycle.calls == [.open("lobby")])
    }

    @Test func aPersonChatHasNoLifecycle() async {
        let lifecycle = RecordingChatRoomLifecycle()
        let context = makeContext(
            chatId: .person(Data(repeating: 1, count: 32)),
            policy: policy(store: .host),
            lifecycle: lifecycle
        )

        await context.open()
        await context.close()

        #expect(lifecycle.calls.isEmpty)
    }
}

private extension ChatRoomSourceContextTests {
    func makeContext(
        chatId: Chat.Id? = nil,
        policy: ChatRoomPolicy,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle(),
        feedListeners: ProductFeedListeners = ProductFeedListeners()
    ) -> ChatRoomSourceContext {
        makeContext(chatId: chatId, policy: { policy }, lifecycle: lifecycle, feedListeners: feedListeners)
    }

    func makeContext(
        chatId: Chat.Id? = nil,
        policy: @escaping @Sendable () -> ChatRoomPolicy,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle(),
        feedListeners: ProductFeedListeners = ProductFeedListeners()
    ) -> ChatRoomSourceContext {
        let chatId = chatId ?? self.chatId
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
            services: ProductRoomServices(
                querying: ScriptedProductChatQuerying(),
                lifecycle: lifecycle,
                feedListeners: feedListeners,
                cache: ProductMessageCache(),
                settings: .init(retryDelay: .milliseconds(1))
            ),
            policy: { policy() },
            logger: StubLogger()
        )
    }

    func policy(store: ChatRoomPolicy.MessageStore) -> ChatRoomPolicy {
        ChatRoomPolicy(messageStore: store, reportsSeen: true, footer: ChatRoomPolicy.defaults.footer)
    }

    func message(_ id: String) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: 1_000,
            body: .text(text: id),
            status: .sent,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }
}
