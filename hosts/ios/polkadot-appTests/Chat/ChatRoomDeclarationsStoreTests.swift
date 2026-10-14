import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// The declarations that a product sets for its rooms persist in the chat store.
struct ChatRoomDeclarationsStoreTests {
    private let fixture = ProductRoomFixture()

    @Test func aConfigurationPersistsAndAChangeBumpsTheGeneration() async throws {
        try await createRoom()

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: reportingSeen)
        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: reportingSeen)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.config == reportingSeen)
        #expect(declarations.configGeneration == 1)
    }

    /// The core sets the default configuration on each new room, and that writes nothing.
    @Test func theDefaultConfigurationOfANewRoomWritesNothing() async throws {
        try await createRoom()

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: .hostDefault)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.config == nil)
        #expect(declarations.configGeneration == 0)
        #expect(declarations.effectiveConfig == .hostDefault)
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

    @Test func theToolbarTheSuggestionsAndTheCommandsPersist() async throws {
        try await createRoom()
        let toolbar = [Chat.RoomToolbarAction(
            actionId: "rules",
            label: "Rules",
            icon: .preimage(key: Data(repeating: 2, count: 32)),
            handler: .callback
        )]
        let suggestions = [Chat.RoomSuggestion(suggestionId: "yes", text: "Yes", sendsText: false)]
        let commands = [Chat.RoomCommand(name: "roll", description: "Roll dice", argumentHint: nil)]

        try await fixture.context.setRoomToolbar(for: fixture.bot, roomId: "lobby", actions: toolbar)
        try await fixture.context.setRoomSuggestions(for: fixture.bot, roomId: "lobby", suggestions: suggestions)
        try await fixture.context.setRoomCommands(for: fixture.bot, roomId: "lobby", commands: commands)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.toolbar == toolbar)
        #expect(declarations.suggestions == suggestions)
        #expect(declarations.commands == commands)
        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.slashCommands == .declared(["roll"]))
    }

    /// The activity line stays in memory: it reaches an observer and goes away with the room.
    @Test func theActivityLineStaysInMemory() async throws {
        try await createRoom()
        let chatId = fixture.chatId("lobby")
        var updates = fixture.activities.activityUpdates(for: chatId).makeAsyncIterator()
        #expect(await updates.next() == .some(nil))

        let typing = Chat.RoomActivity(text: "Bot is typing", participantId: nil)
        try await fixture.context.setRoomActivity(for: fixture.bot, roomId: "lobby", activity: typing)
        #expect(await updates.next() == typing)
        #expect(fixture.activities.activity(for: chatId) == typing)

        try await fixture.context.deleteRoom(for: fixture.bot, roomId: "lobby")
        #expect(await updates.next() == .some(nil))
        #expect(fixture.activities.activity(for: chatId) == nil)
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
        await #expect(throws: ChatRoomError.self) { try await context.setRoomToolbar(
            for: bot,
            roomId: "gone",
            actions: []
        ) }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomSuggestions(for: bot, roomId: "gone", suggestions: [])
        }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomCommands(for: bot, roomId: "gone", commands: [])
        }
        await #expect(throws: ChatRoomError.self) {
            try await context.setRoomActivity(for: bot, roomId: "gone", activity: nil)
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
            participantActions: false,
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
            participantActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
    }

    func createRoom() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
    }
}
