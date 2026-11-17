import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The declarations that a product sets for its rooms persist in the chat store.
struct ChatRoomDeclarationsStoreTests {
    private let fixture = ProductRoomFixture()

    /// Each `set_room_config` starts a new generation, also with an unchanged
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

    /// A change of the message store discards the messages that the host kept.
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

    @Test func eachDeclarationRefusesARoomThatDoesNotExist() async {
        let context = fixture.context
        let bot = fixture.bot

        await #expect(throws: ChatRoomError.self) { try await context.roomDeclarations(for: bot, roomId: "gone") }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomConfig(for: bot, roomId: "gone", config: .hostDefault)
        }
    }
}

private extension ChatRoomDeclarationsStoreTests {
    var reportingSeen: Chat.RoomConfig {
        Chat.RoomConfig(productSource: false, reactionDetails: false, reportSeen: true)
    }

    var productSource: Chat.RoomConfig {
        Chat.RoomConfig(productSource: true, reactionDetails: false, reportSeen: false)
    }

    func createRoom() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
    }
}
