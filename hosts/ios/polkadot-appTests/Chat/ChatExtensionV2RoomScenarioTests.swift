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
            metadata: ChatRoomMetadata(name: "Lobby", description: nil),
            config: .hostDefault,
            preview: nil
        )

        #expect(status == .new)
        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomMetadata == Chat.RoomMetadata(chatRelativeId: "lobby", name: "Lobby", icon: icon))
        // The core sets the configuration of a new room, and each `set_room_config` starts a generation (O9).
        var defaults = ChatRoomPolicy.defaults
        defaults.configGeneration = 1
        #expect(chat.roomPolicy == defaults)
        #expect(try await bridge.roomConfig(roomId: "lobby") == .hostDefault)
        #expect(try await bridge.listRooms() == [ChatRoom(roomId: "lobby", participatingAs: .roomHost)])
    }

    /// S2: a second-version `create_room` keeps its configuration and preview, and a repeat that finds the room
    /// keeps them.
    @Test func s2ASecondVersionRoomKeepsItsDeclarations() async throws {
        let bridge = await fixture.bridge()

        let first = try await create(bridge, metadata: metadata, config: config, preview: preview)
        let declared = try await fixture.storedChat("lobby")?.roomDeclarations
        let repeated = try await create(
            bridge,
            metadata: ChatRoomMetadata(name: "Lobby", description: "Another"),
            config: .hostDefault,
            preview: nil
        )

        #expect(first == .new)
        #expect(repeated == .exists)
        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomDeclarations == declared)
        #expect(chat.roomDeclarations?.config == Chat.RoomConfig(config))
        #expect(chat.roomDeclarations?.preview == Chat.RoomPreview(preview))
        #expect(chat.roomPolicy?.reportsSeen == true)
        #expect(try await bridge.roomConfig(roomId: "lobby") == config)
    }

    /// S3: `set_room_metadata` reaches the chat-list row and the room header. The native chat draws no
    /// description.
    @Test func s3MetadataReachesTheListAndTheHeader() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: .hostDefault, preview: nil)

        try await bridge.setRoomMetadata(roomId: "lobby", metadata: ChatRoomMetadata(name: "Finals", description: "x"))

        let chat = try #require(try await fixture.storedChat("lobby"))
        let registry = SingleBotRegistry(bot: fixture.bot)
        #expect(chat.chatWithPeerMetadata(using: registry).peerMetadata.name == "Finals")
        #expect(chat.chatMetadata(using: registry).peerMetadata.name == "Finals")
    }

    /// S4: the badge, the time, the last message and the reaction of a preview reach the row of the chat list.
    @Test func s4ThePreviewReachesTheRow() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: .hostDefault, preview: nil)

        try await bridge.setRoomPreview(roomId: "lobby", preview: ChatRoomPreview(
            timestamp: 42,
            badge: .unreadCount(count: 3),
            // The product gives a message of the contact that the user has not seen as `Delivered`.
            lastMessage: message("m1", text: "Your turn", timestamp: 42, status: .delivered),
            lastReaction: nil,
            unseenReactions: 1
        ))
        var chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomPolicy?.listBadge == .count(3))
        #expect(chat.unreadDisplayMessageCount == 3)
        #expect(chat.hasIncomingReaction)
        #expect(chat.message?.content == .text("Your turn"))
        #expect(chat.message?.timestamp == 42)
        #expect(chat.message?.status == .incoming(.new))

        try await bridge.setRoomPreview(roomId: "lobby", preview: ChatRoomPreview(
            timestamp: nil,
            badge: .hidden,
            lastMessage: nil,
            lastReaction: nil,
            unseenReactions: 0
        ))
        chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomPolicy?.listBadge == .hidden)
        #expect(chat.unreadDisplayMessageCount == 0)
        #expect(!chat.hasIncomingReaction)
        #expect(chat.message == nil)
    }

    /// S5: `delete_room` removes the room, its messages and reactions, and its declarations. A second delete
    /// succeeds.
    @Test func s5DeleteRemovesTheRoomAndEverythingInIt() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: config, preview: preview)
        try await bridge.updateFeed(roomId: "lobby", update: .new(message("m1", text: "hi")), persist: true)
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .reactionPlaced(messageId: "m1", participantId: "alice", emoji: "👍"),
            persist: true
        )

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
        try await create(bridge, metadata: metadata, config: config, preview: preview)

        let workers = StubWorkerManager()
        let productBot = ProductBot(
            product: Product(id: fixture.productId, name: "Game"),
            description: nil,
            runtime: TrUAPIChatHandler(productId: fixture.productId, workers: workers),
            resolveImage: WidgetImageResolver { _ in nil }
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
        #expect(row.chat.message?.content == .text("Your turn"))
        #expect(workers.references.acquired.isEmpty)
        await productBot.dispose()
    }

    /// The contact of a room survives a restart, and the room draws as the native person chat with the contact.
    @Test func theContactOfARoomDrawsThePersonChat() async throws {
        let bridge = await fixture.bridge()
        try await create(bridge, metadata: metadata, config: config, preview: nil)
        let account = Data(repeating: 7, count: 32)

        try await bridge.setRoomContact(
            roomId: "lobby",
            contact: ChatRoomContact(account: account, username: "alice", state: .established)
        )

        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomContact == Chat.RoomContact(account: account, username: "alice", state: .established))
        #expect(chat.avatarColorSeed == Chat.Id.person(account).colorSeed)
        let header = chat.chatMetadata(using: SingleBotRegistry(bot: fixture.bot))
        #expect(header.peerMetadata.name == "alice")
        #expect(header.peerMetadata.icon == .image(nil))
        #expect(header.peerMetadata.moreActions == [.blockUser, .leaveChat])
        #expect(header.colorSeed == account.toHex())
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
        #expect(opened.roomPolicy?.reportsSeen == true)

        var changed = config
        changed.reportSeen = false
        try await bridge.setRoomConfig(roomId: "lobby", config: changed)

        var reloaded = opened
        while reloaded.roomPolicy?.configGeneration == generation {
            reloaded = try #require(try await chats.next() ?? nil)
        }

        #expect(reloaded.roomPolicy?.configGeneration == generation + 1)
        #expect(reloaded.roomPolicy?.reportsSeen == false)
    }
}

private let png = Data([0x89, 0x50, 0x4E, 0x47])

private extension ChatExtensionV2RoomScenarioTests {
    var metadata: ChatRoomMetadata {
        ChatRoomMetadata(name: "Lobby", description: "Weekly game")
    }

    var config: ChatRoomConfig {
        ChatRoomConfig(productSource: false, reactionDetails: false, reportSeen: true)
    }

    var preview: ChatRoomPreview {
        ChatRoomPreview(
            timestamp: nil,
            badge: nil,
            lastMessage: message("m0", text: "Your turn"),
            lastReaction: nil,
            unseenReactions: 0
        )
    }

    func message(
        _ id: String,
        text: String,
        timestamp: UInt64 = 1,
        status: ChatMessageStatus = .sent
    ) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: timestamp,
            body: .text(text: text),
            status: status,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
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
