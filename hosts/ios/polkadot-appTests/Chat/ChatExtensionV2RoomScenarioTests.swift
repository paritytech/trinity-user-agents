import Foundation
import Operation_iOS
import PolkadotUI
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Scenarios S1 to S6 of 04-TESTING for the iOS host (P3.8): the rust chat bridge on a real chat store, driven
/// the way the core drives it. For a new room the core calls `create_room` and then `set_room_metadata`,
/// `set_room_config` and, when the request has one, `set_room_preview` (02-DESIGN §14 item 11).
struct ChatExtensionV2RoomScenarioTests {
    private let fixture = ProductRoomFixture()

    /// S1: a first-version `create_room` gives a room that the host stores, with the defaults of §4.
    @Test func s1AFirstVersionRoomHasTheDefaults() async throws {
        let bridge = await fixture.bridge()
        let icon = "data:image/png;base64," + png.base64EncodedString()

        let status = try await create(
            bridge,
            icon: icon,
            metadata: ChatRoomMetadata(
                name: "Lobby",
                avatar: .inline(mimeType: "image/png", bytes: png),
                description: nil
            ),
            config: .hostDefault,
            preview: nil
        )

        #expect(status == .new)
        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomMetadata == Chat.RoomMetadata(chatRelativeId: "lobby", name: "Lobby", icon: icon))
        #expect(chat.roomDeclarations?.avatar == .inline(png))
        #expect(chat.roomPolicy == .defaults)
        #expect(try await bridge.roomConfig(roomId: "lobby") == .hostDefault)
        #expect(try await bridge.listRooms() == [ChatRoom(roomId: "lobby", participatingAs: .roomHost)])
    }

    /// S2: a second-version `create_room` keeps its metadata, configuration and preview, and a repeat that
    /// finds the room keeps them.
    @Test func s2ASecondVersionRoomKeepsItsDeclarations() async throws {
        let bridge = await fixture.bridge()

        let first = try await create(bridge, metadata: metadata, config: config, preview: pinned)
        let declared = try await fixture.storedChat("lobby")?.roomDeclarations
        let repeated = try await create(
            bridge,
            metadata: ChatRoomMetadata(name: "Lobby", avatar: nil, description: "Another"),
            config: .hostDefault,
            preview: nil
        )

        #expect(first == .new)
        #expect(repeated == .exists)
        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomDeclarations == declared)
        #expect(chat.roomDeclarations?.description == "Weekly game")
        #expect(chat.roomDeclarations?.avatar == .preimage(key: key))
        #expect(chat.roomDeclarations?.config == Chat.RoomConfig(config))
        #expect(chat.roomDeclarations?.preview == Chat.RoomPreview(pinned))
        #expect(chat.roomPolicy?.reportsSeen == true)
        #expect(chat.roomPolicy?.listOrder.isPinned == true)
        #expect(try await bridge.roomConfig(roomId: "lobby") == config)
    }

    /// S3: `set_room_metadata` reaches the chat-list row and the room header.
    @Test func s3MetadataReachesTheListAndTheHeader() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: .hostDefault, preview: nil)

        try await bridge.setRoomMetadata(roomId: "lobby", metadata: ChatRoomMetadata(
            name: "Finals",
            avatar: .inline(mimeType: "image/png", bytes: png),
            description: "Tonight"
        ))

        let chat = try #require(try await fixture.storedChat("lobby"))
        let registry = SingleBotRegistry(bot: fixture.bot)
        let row = chat.chatWithPeerMetadata(using: registry).peerMetadata
        #expect(row.name == "Finals")
        #expect(row.icon == .roomImage(.inline(png)))
        let header = chat.chatMetadata(using: registry)
        #expect(header.peerMetadata.name == "Finals")
        #expect(header.subtitle == "Tonight")
    }

    /// S4: each preview variant reaches the policy that the chat list reads, and a preview without values
    /// gives the host defaults back.
    @Test(arguments: PreviewCase.all)
    func s4EachPreviewReachesTheList(_ previewCase: PreviewCase) async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: .hostDefault, preview: pinned)

        try await bridge.setRoomPreview(roomId: "lobby", preview: previewCase.wire)

        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomDeclarations?.preview == previewCase.stored)
        #expect(chat.roomPolicy == ChatRoomPolicy(footer: nil, preview: previewCase.stored))
    }

    @Test func s4ThePolicyShowsEachPreviewValue() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: .hostDefault, preview: nil)

        try await bridge.setRoomPreview(roomId: "lobby", preview: ChatRoomPreview(
            order: .pinToTop,
            timestamp: 42,
            badge: .unreadCount(count: 3),
            text: ChatRoomPreviewText(text: "Bot is typing", ongoing: true)
        ))
        var policy = try #require(try await fixture.storedChat("lobby")?.roomPolicy)
        #expect(policy.listOrder == .init(isPinned: true, timestamp: 42))
        #expect(policy.listBadge == .count(3))
        #expect(policy.listPreview == .text("Bot is typing", ongoing: true))

        try await bridge.setRoomPreview(roomId: "lobby", preview: ChatRoomPreview(
            order: .timestamp,
            timestamp: nil,
            badge: .image(resource: .preimage(key: key)),
            text: nil
        ))
        policy = try #require(try await fixture.storedChat("lobby")?.roomPolicy)
        #expect(policy.listOrder == .lastMessage)
        #expect(policy.listBadge == .image(.preimage(key: key)))
        #expect(policy.listPreview == .lastMessage)
    }

    /// S5: `delete_room` removes the room, its messages and reactions, and its declarations. A second delete
    /// succeeds.
    @Test func s5DeleteRemovesTheRoomAndEverythingInIt() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: config, preview: pinned)
        let message = ChatMessage(
            messageId: "m1",
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: 1,
            body: .text(text: "hi"),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
        try await bridge.updateFeed(roomId: "lobby", update: .new(message), persist: true)
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .reactionPlaced(messageId: "m1", participantId: "alice", emoji: "👍"),
            persist: true
        )
        try await bridge.setRoomCommands(roomId: "lobby", commands: [
            ChatCommandSpec(name: "roll", description: "Roll", argumentHint: nil)
        ])

        try await bridge.deleteRoom(roomId: "lobby")
        try await bridge.deleteRoom(roomId: "lobby")

        #expect(try await fixture.storedChat("lobby") == nil)
        #expect(try await fixture.messages("lobby").isEmpty)
        #expect(try await reactionCount() == 0)
        #expect(try await bridge.listRooms().isEmpty)
        await #expect(throws: ChatRoomError.self) { try await bridge.roomConfig(roomId: "lobby") }
    }

    /// S6: the chat list draws a product room from what the store holds, and the bot of the product asks for
    /// no worker to do so.
    @Test func s6TheListDrawsARoomWithoutTheWorker() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: config, preview: pinned)

        let workers = StubWorkerManager()
        let productBot = ProductBot(
            product: Product(id: fixture.productId, name: "Game"),
            description: nil,
            runtime: TrUAPIChatHandler(productId: fixture.productId, workers: workers),
            resolveImage: WidgetImageResolver { _ in nil },
            rooms: ProductRoomReader(storageFacade: fixture.storageFacade)
        )

        let chats = try await fixture.storageFacade.createRepository(
            filter: .roomChatsForExtension(fixture.productId),
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatModelMapper())
        )
        .fetchAllOperation(with: RepositoryFetchOptions())
        .asyncExecute()

        let row = try #require(chats.first).chatWithPeerMetadata(using: SingleBotRegistry(bot: productBot))
        #expect(row.peerMetadata.name == "Lobby")
        #expect(row.peerMetadata.icon == .roomImage(.preimage(key: key)))
        #expect(row.chat.roomPolicy?.listPreview == .text("Your turn", ongoing: false))
        #expect(row.chat.roomPolicy?.listOrder.isPinned == true)
        #expect(workers.references.acquired.isEmpty)
        await productBot.dispose()
    }

    /// S47: a `set_room_config` while the room screen is open reaches the screen with a new configuration
    /// generation, so the screen reloads its policy without a reopen. The screen reads its policy from the chat
    /// stream that `ChatEngine.chatMetadataStream()` follows.
    @Test func s47AConfigurationChangeReachesAnOpenScreen() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: config, preview: nil)

        let provider = ChatContactDataProviderFactory(
            repositoryFactory: ChatContactRepositoryFactory(storageFacade: fixture.storageFacade)
        )
        var chats = provider.subscribeChat(by: fixture.chatId("lobby")).makeAsyncIterator()
        let opened = try #require(try await chats.next() ?? nil)
        let generation = try #require(opened.roomPolicy?.configGeneration)
        #expect(opened.roomPolicy?.messageMenu.allowedReactions == ["👍"])

        var changed = config
        changed.allowedReactions = ["❤️", "🎉"]
        try await bridge.setRoomConfig(roomId: "lobby", config: changed)

        var reloaded = opened
        while reloaded.roomPolicy?.configGeneration == generation {
            reloaded = try #require(try await chats.next() ?? nil)
        }

        #expect(reloaded.roomPolicy?.configGeneration == generation + 1)
        #expect(reloaded.roomPolicy?.messageMenu.allowedReactions == ["❤️", "🎉"])
    }
}

private let png = Data([0x89, 0x50, 0x4E, 0x47])
private let key = Data(repeating: 9, count: 32)

private extension ChatExtensionV2RoomScenarioTests {
    var metadata: ChatRoomMetadata {
        ChatRoomMetadata(name: "Lobby", avatar: .preimage(key: key), description: "Weekly game")
    }

    var config: ChatRoomConfig {
        ChatRoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: ["👍"],
            placeholder: "Nothing yet",
            reportSeen: true
        )
    }

    var pinned: ChatRoomPreview {
        ChatRoomPreview(
            order: .pinToTop,
            timestamp: nil,
            badge: nil,
            text: ChatRoomPreviewText(text: "Your turn", ongoing: false)
        )
    }

    /// The calls of the core for one `create_room`.
    @discardableResult
    func create(
        _ bridge: RustChatExecutionBridge,
        icon: String = "",
        metadata: ChatRoomMetadata,
        config: ChatRoomConfig,
        preview: ChatRoomPreview?
    ) async throws -> ChatRoomRegistrationStatus {
        let status = try await bridge.createRoom(roomId: "lobby", name: metadata.name, icon: icon)

        if status == .new {
            try await bridge.setRoomMetadata(roomId: "lobby", metadata: metadata)
            try await bridge.setRoomConfig(roomId: "lobby", config: config)
            if let preview {
                try await bridge.setRoomPreview(roomId: "lobby", preview: preview)
            }
        }

        return status
    }

    func reactionCount() async throws -> Int {
        try await fixture.storageFacade.databaseService.performRead { context in
            try context.count(for: CDChatMessageReaction.fetchRequest())
        }
    }
}
