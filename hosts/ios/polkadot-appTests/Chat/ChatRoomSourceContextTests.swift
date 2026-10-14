import Foundation
import Operation_iOS
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The engine picks the message source of a room from its policy (P5.2).
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

    @Test func theProductSourceListensToTheFeedOfItsRoom() async throws {
        let registry = ProductFeedListenerRegistry()
        let context = makeContext(policy: policy(store: .product), feedRegistry: registry)
        let source = try #require(await context.source() as? ProductMessageSource)
        await source.start()
        await source.waitForFirstPage()

        #expect(registry.apply(productId: "product.dot", roomId: "lobby", update: .new(message("m1"))))
        #expect(!registry.apply(productId: "product.dot", roomId: "other", update: .new(message("m2"))))

        for _ in 0 ..< 500 where await source.snapshot.messages.isEmpty {
            try await Task.sleep(for: .milliseconds(2))
        }
        #expect(await source.snapshot.messages.map(\.messageId) == ["m1"])
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
}

private extension ChatRoomSourceContextTests {
    func makeContext(
        chatId: Chat.Id? = nil,
        policy: ChatRoomPolicy,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle(),
        feedRegistry: ProductFeedListenerRegistry = ProductFeedListenerRegistry()
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
            messageDeletionRepository: AnyDataProviderRepository(facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageDeletionMapper())
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
                feedRegistry: feedRegistry,
                cache: ProductMessageCache(),
                settings: .init(retryDelay: .milliseconds(1))
            ),
            policy: { policy },
            logger: StubLogger()
        )
    }

    func policy(store: ChatRoomPolicy.MessageStore) -> ChatRoomPolicy {
        let defaults = ChatRoomPolicy.defaults
        return ChatRoomPolicy(
            messageStore: store,
            listPreview: defaults.listPreview,
            listBadge: defaults.listBadge,
            listOrder: defaults.listOrder,
            reportsSeen: true,
            messageMenu: defaults.messageMenu,
            participants: defaults.participants,
            offersReactionDetails: defaults.offersReactionDetails,
            toolbar: defaults.toolbar,
            footer: defaults.footer,
            placeholder: defaults.placeholder,
            suggestions: defaults.suggestions,
            slashCommands: defaults.slashCommands,
            activity: defaults.activity,
            hostAppliesUserActions: store == .host
        )
    }

    func message(_ id: String) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: 1_000,
            body: .text(text: id),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }
}
