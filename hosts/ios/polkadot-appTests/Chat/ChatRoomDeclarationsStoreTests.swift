import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The declarations that a product sets for its rooms persist in the chat store.
struct ChatRoomDeclarationsStoreTests {
    private let fixture = ProductRoomFixture()

    /// 02-DESIGN §8.2 (O9, PARITY row 1): each `set_room_config` starts a new generation, also with an unchanged
    /// configuration, as on Android.
    @Test func eachConfigurationPersistsAndStartsANewGeneration() async throws {
        try await createRoom()

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: reportingSeen)
        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: reportingSeen)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.config == reportingSeen)
        #expect(declarations.configGeneration == 2)
    }

    /// The core sets the configuration of each new room. The default configuration also starts a generation and
    /// keeps the messages.
    @Test func theDefaultConfigurationOfANewRoomStartsAGeneration() async throws {
        try await createRoom()
        try await fixture.save(fixture.userMessage(.text("hello"), roomId: "lobby"))

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: .hostDefault)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.config == .hostDefault)
        #expect(declarations.configGeneration == 1)
        #expect(declarations.effectiveConfig == .hostDefault)
        #expect(try await fixture.messages("lobby").count == 1)
    }

    /// 02-DESIGN §13 item 8: a change of the message store discards the messages that the host kept.
    @Test func aChangeOfTheMessageStoreDiscardsTheMessages() async throws {
        try await createRoom()
        try await fixture.save(fixture.userMessage(.text("hello"), roomId: "lobby"))
        try await fixture.save(.newExtensionMessage(fixture.bot.identifier, roomId: "lobby", content: .text("hi")))
        #expect(try await fixture.messages("lobby").count == 2)

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: productSource)

        #expect(try await fixture.messages("lobby").isEmpty)
        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.message == nil)
        #expect(chat.unreadDisplayMessageCount == 0)
        #expect(chat.roomMetadata?.name == "Lobby")
        #expect(chat.roomPolicy?.messageStore == .product)
    }

    @Test func aChangeThatKeepsTheMessageStoreKeepsTheMessages() async throws {
        try await createRoom()
        try await fixture.save(fixture.userMessage(.text("hello"), roomId: "lobby"))

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: reportingSeen)

        #expect(try await fixture.messages("lobby").count == 1)
    }

    @Test func aFooterKeepsTheFooterOfTheFirstVersionInStep() async throws {
        try await createRoom()

        try await fixture.context.setRoomFooterConfig(for: fixture.bot, roomId: "lobby", footer: .empty)
        var chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomFooter == .empty)
        #expect(chat.roomDeclarations?.footerConfig == .empty)

        let input = Chat.RoomInputConfig(placeholder: "Ask", photo: true, video: false, file: false, payment: nil)
        try await fixture.context.setRoomFooterConfig(for: fixture.bot, roomId: "lobby", footer: .input(input))
        chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomFooter == .textInput)
        #expect(chat.roomDeclarations?.footerConfig == .input(input))
    }

    @Test func eachDeclarationRefusesARoomThatDoesNotExist() async {
        let context = fixture.context
        let bot = fixture.bot

        await #expect(throws: ChatRoomError.self) { try await context.roomDeclarations(for: bot, roomId: "gone") }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomConfig(for: bot, roomId: "gone", config: .hostDefault)
        }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomFooterConfig(for: bot, roomId: "gone", footer: .empty)
        }
    }
}

private extension ChatRoomDeclarationsStoreTests {
    var reportingSeen: Chat.RoomConfig {
        Chat.RoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: true
        )
    }

    var productSource: Chat.RoomConfig {
        Chat.RoomConfig(
            productSource: true,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
    }

    func createRoom() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
    }
}

/// The bot of a product room reads the declarations and the newest seen message from the store.
struct ProductRoomReaderTests {
    private let fixture = ProductRoomFixture()

    @Test func theReaderGivesTheDeclarationsOfAStoredRoomOnly() async throws {
        let reader = ProductRoomReader(storageFacade: fixture.storageFacade)
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        let features = Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, readAloud: true)
        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: features)

        #expect(await reader.declarations(of: fixture.chatId("lobby"))?.features == features)
        #expect(await reader.declarations(of: fixture.chatId("gone")) == nil)
    }

    @Test func theNewestMessageFollowsTheOrderOfTheRoom() async throws {
        let reader = ProductRoomReader(storageFacade: fixture.storageFacade)
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        for id in ["a", "b", "c"] {
            try await fixture.save(fixture.userMessage(.text(id), roomId: "lobby", id: id))
        }

        #expect(await reader.newestMessageId(among: ["a", "b"], in: fixture.chatId("lobby")) == "b")
        #expect(await reader.newestMessageId(among: ["c", "a"], in: fixture.chatId("lobby")) == "c")
        #expect(await reader.newestMessageId(among: ["x"], in: fixture.chatId("lobby")) == nil)
        #expect(await reader.newestMessageId(among: [], in: fixture.chatId("lobby")) == nil)
    }
}
