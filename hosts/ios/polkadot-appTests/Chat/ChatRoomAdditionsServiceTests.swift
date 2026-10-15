import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The services of the room screen for the Chat Extension v2 additions (07-LATER §5, §9): personas, meta updates,
/// reasoning rows in the chat store, `Preparing` messages, read aloud and the thread cascade of `delete_room`.
struct ChatRoomAdditionsServiceTests {
    private let fixture = ProductRoomFixture()

    // MARK: - Personas

    @Test func theRoomScreenFollowsThePersonasOfItsProduct() async throws {
        let store = ChatProductPersonaStore(storageFacade: fixture.storageFacade)
        var updates = store.personaUpdates(of: fixture.productId).makeAsyncIterator()
        #expect(try await updates.next() == [])

        _ = try await store.register(persona("helper", name: "Helper"))

        #expect(try await updates.next()?.map(\.name) == ["Helper"])
    }

    @Test func aPersonaIdIsBotAndTheBotId() {
        #expect(Chat.ProductPersona.botId(ofParticipant: "bot:helper") == "helper")
        #expect(Chat.ProductPersona.botId(ofParticipant: "bot:") == nil)
        #expect(Chat.ProductPersona.botId(ofParticipant: "p1") == nil)
        #expect(persona("helper", name: "Helper").participantId == "bot:helper")
    }

    // MARK: - Meta

    @Test func theRoomScreenFollowsTheMetaOfItsRoom() async throws {
        try await createRoom("lobby")
        try await post("r1", text: "thinking", roomId: "lobby")
        let store = ChatMessageMetaStore(databaseService: fixture.storageFacade.databaseService)
        var updates = store.metaUpdates(for: fixture.chatId("lobby")).makeAsyncIterator()
        #expect(try await updates.next()?.isEmpty == true)

        try await store.setMeta(branch, productMessageId: "r1", chatId: fixture.chatId("lobby"))

        #expect(try await updates.next()?.meta(for: "r1") == branch)
    }

    /// S66: a reasoning row is not the chat-list preview and is not unread.
    @Test func aReasoningRowIsNotThePreviewAndIsNotUnread() async throws {
        try await createRoom("lobby")
        try await post("p1", text: "An answer", roomId: "lobby")
        try await post("r1", text: "The user asked", roomId: "lobby")
        #expect(try await previewText("lobby") == "The user asked")

        try await fixture.context.setMessageMeta(for: fixture.bot, roomId: "lobby", messageId: "r1", meta: reasoning)

        #expect(try await previewText("lobby") == "An answer")
        let stored = try await fixture.messages("lobby").first { $0.content == .text("The user asked") }
        #expect(stored?.status == .incoming(.seen))
    }

    @Test func aDeletedMessageLosesItsMeta() async throws {
        try await createRoom("lobby")
        try await post("r1", text: "thinking", roomId: "lobby")
        try await fixture.context.setMessageMeta(for: fixture.bot, roomId: "lobby", messageId: "r1", meta: branch)

        try await fixture.context.applyFeedUpdate(for: fixture.bot, roomId: "lobby", update: .deleted(messageId: "r1"))

        let index = try await ChatMessageMetaStore(databaseService: fixture.storageFacade.databaseService)
            .metaIndex(for: fixture.chatId("lobby"))
        #expect(index.isEmpty)
    }

    // MARK: - Preparing messages

    @Test func theBridgeRecordsWhichMessagesArePreparing() async throws {
        try await createRoom("lobby")
        let preparing = ChatPreparingMessageStore()
        let productId = fixture.productId
        let bridge = await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies(productId: productId) },
            chatMessaging: fixture.surface,
            preparingMessages: preparing
        )

        try await bridge.updateFeed(roomId: "lobby", update: .new(wireMessage("r1")), persist: true)
        #expect(preparing.preparingMessageIds(in: fixture.chatId("lobby")) == ["r1"])

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .statusChanged(messageId: "r1", status: .sent),
            persist: true
        )
        #expect(preparing.preparingMessageIds(in: fixture.chatId("lobby")).isEmpty)
    }

    @Test func aDeletedMessageIsNotPreparing() {
        let preparing = ChatPreparingMessageStore()
        let chatId = fixture.chatId("lobby")
        preparing.setPreparing(true, productMessageId: "r1", chatId: chatId)

        preparing.record(.deleted(messageId: "r1"), isPreparing: false, chatId: chatId)

        #expect(preparing.preparingMessageIds(in: chatId).isEmpty)
    }

    // MARK: - Read aloud

    @Test func readAloudSpeaksMarkdownWithoutItsMarks() {
        let markdown = "# Rich text sample\n\nA paragraph with **bold**, *italic* and `code`.\n\n"
            + "- first item\n> A quote"

        #expect(ChatReadAloudService.plainText(fromMarkdown: markdown)
            == "Rich text sample\n\nA paragraph with bold, italic and code.\n\nfirst item\nA quote")
        #expect(ChatReadAloudService.plainText(fromMarkdown: "plain") == "plain")
    }

    // MARK: - Threads

    /// S64: `delete_room` of a parent also deletes its thread rooms, and leaves the other rooms alone.
    @Test func deletingARoomDeletesItsThreadRooms() async throws {
        try await createRoom("lobby")
        try await createRoom("thread-lobby-m1", threadOf: .init(roomId: "lobby", messageId: "m1"))
        try await createRoom("other")
        try await createRoom("thread-other-m1", threadOf: .init(roomId: "other", messageId: "m1"))

        try await fixture.context.deleteRoom(for: fixture.bot, roomId: "lobby")

        #expect(try await fixture.storedChat("lobby") == nil)
        #expect(try await fixture.storedChat("thread-lobby-m1") == nil)
        #expect(try await fixture.storedChat("other") != nil)
        #expect(try await fixture.storedChat("thread-other-m1") != nil)
    }

    @Test func aThreadRootComesFromTheParentRoomOfTheHostStore() async throws {
        try await createRoom("lobby")
        try await post("m1", text: "The root", roomId: "lobby")
        let resolver = ChatThreadRootResolver(storageFacade: fixture.storageFacade, pageCache: ProductMessageCache())

        let root = await resolver.root(
            of: .init(roomId: "lobby", messageId: "m1"),
            threadChatId: fixture.chatId("thread-lobby-m1")
        )
        let missing = await resolver.root(
            of: .init(roomId: "lobby", messageId: "gone"),
            threadChatId: fixture.chatId("thread-lobby-gone")
        )

        #expect(root == ChatThreadRoot(parentName: "lobby", author: "lobby", text: "The root"))
        #expect(missing == ChatThreadRoot(parentName: "lobby", author: nil, text: nil))
    }
}

private extension ChatRoomAdditionsServiceTests {
    var reasoning: Chat.MessageMeta {
        Chat.MessageMeta(thread: nil, branch: nil, reasoning: .init(durationMs: 4_000))
    }

    var branch: Chat.MessageMeta {
        Chat.MessageMeta(thread: nil, branch: .init(index: 1, count: 3), reasoning: nil)
    }

    func persona(_ botId: String, name: String) -> Chat.ProductPersona {
        Chat.ProductPersona(productId: fixture.productId, botId: botId, name: name, icon: nil)
    }

    func createRoom(_ roomId: String, threadOf: Chat.ThreadParent? = nil) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
        guard let threadOf else { return }
        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: roomId, features: Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: threadOf,
            readAloud: false
        ))
    }

    func post(_ messageId: String, text: String, roomId: String) async throws {
        try await fixture.context.applyFeedUpdate(
            for: fixture.bot,
            roomId: roomId,
            update: .new(Chat.ProductFeedMessage(
                messageId: messageId,
                participantId: "product",
                timestamp: Date().toChatTimestamp(),
                content: .text(text),
                replyTo: nil
            ))
        )
    }

    func previewText(_ roomId: String) async throws -> String? {
        guard case let .text(text) = try await fixture.storedChat(roomId)?.message?.content else { return nil }
        return text
    }

    func wireMessage(_ id: String) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: 1_700_000_000_000,
            body: .text(text: "thinking"),
            status: .preparing,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }
}
