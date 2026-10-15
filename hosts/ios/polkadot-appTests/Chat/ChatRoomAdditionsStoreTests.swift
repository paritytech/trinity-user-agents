import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
@testable import polkadot_app

/// The chat store keeps what a product sets through the Chat Extension v2 additions (07-LATER §4.4, §9.6).
struct ChatRoomAdditionsStoreTests {
    private let fixture = ProductRoomFixture()

    @Test func theFeaturesReachThePolicyOfTheRoom() async throws {
        try await createRoom("lobby")
        let features = Chat.RoomFeatures(
            preimageAttachments: false,
            search: true,
            calls: Chat.RoomCalls(audio: true, video: true),
            threadOf: nil,
            readAloud: true
        )

        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: features)

        #expect(try await fixture.context.roomFeatures(for: fixture.bot, roomId: "lobby") == features)
        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.features == features)
    }

    @Test func theFeaturesLeaveTheOtherDeclarationsAlone() async throws {
        try await createRoom("lobby")
        let preview = Chat.RoomPreview(order: .pinToTop, timestamp: 9, badge: .none, text: "Hi", ongoing: false)
        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: "lobby", preview: preview)

        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: readAloud)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.preview == preview)
        #expect(declarations.features == readAloud)
    }

    /// A room that the product keeps holds the meta of its newest messages only.
    @Test func aProductSourceRoomKeepsTheNewestMeta() async throws {
        try await createRoom("lobby")
        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: productSource)
        let clock = TestClock()
        let store = ChatMessageMetaStore(
            databaseService: fixture.storageFacade.databaseService,
            keptMax: 2,
            now: clock.next
        )

        for messageId in ["m1", "m2", "m3"] {
            try await store.setMeta(reasoning, productMessageId: messageId, chatId: fixture.chatId("lobby"))
        }

        let index = try await store.metaIndex(for: fixture.chatId("lobby"))
        #expect(index.count == 2)
        #expect(index.meta(for: "m1") == nil)
        #expect(index.meta(for: "m3") == reasoning)
    }

    @Test func aTombstoneHasNoMeta() async throws {
        try await createRoom("lobby")
        let message = fixture.userMessage(.text("hi"), roomId: "lobby", id: "u1")
        try await fixture.save(message)
        try await fixture.context.applyFeedUpdate(for: fixture.bot, roomId: "lobby", update: .deleted(messageId: "u1"))

        await #expect(throws: Chat.ProductFeedError.unknownMessage) {
            try await fixture.context.setMessageMeta(
                for: fixture.bot,
                roomId: "lobby",
                messageId: "u1",
                meta: reasoning
            )
        }
    }

    @Test func aMessageOfTheUserTakesMetaUnderItsHostId() async throws {
        try await createRoom("lobby")
        try await fixture.save(fixture.userMessage(.text("hi"), roomId: "lobby", id: "u1"))

        try await fixture.context.setMessageMeta(for: fixture.bot, roomId: "lobby", messageId: "u1", meta: reasoning)

        #expect(try await metaIndex("lobby").meta(for: "u1") == reasoning)
    }

    @Test func aRemovedRoomRemovesItsMetaAndItsCall() async throws {
        try await createRoom("lobby")
        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: productSource)
        try await fixture.context.setMessageMeta(for: fixture.bot, roomId: "lobby", messageId: "p1", meta: reasoning)
        try await fixture.context.setRoomCall(
            for: fixture.bot,
            roomId: "lobby",
            call: .ringing(callId: "c1", video: false, caller: nil)
        )

        try await fixture.context.deleteRoom(for: fixture.bot, roomId: "lobby")

        #expect(try await metaRowCount() == 0)
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == nil)
    }

    /// The meta names messages of the store that the room leaves (02-DESIGN §13 item 8).
    @Test func aChangeOfTheMessageStoreDiscardsTheMeta() async throws {
        try await createRoom("lobby")
        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: productSource)
        try await fixture.context.setMessageMeta(for: fixture.bot, roomId: "lobby", messageId: "p1", meta: reasoning)

        try await fixture.context.setRoomConfig(for: fixture.bot, roomId: "lobby", config: .hostDefault)

        #expect(try await metaRowCount() == 0)
    }

    @Test func theUserStatesNameOnlyTheRoomsOfThisProduct() async throws {
        try await createRoom("lobby")
        try await fixture.context.saveDeclaration(Chat.RoomUserStateUpdate(
            chatId: fixture.chatId("lobby"),
            state: Chat.RoomUserState(mutedUntil: 5, archived: true)
        ))

        let states = try await fixture.context.roomUserStates(for: fixture.bot)

        #expect(states == [Chat.RoomUserStateEntry(roomId: "lobby", state: .init(mutedUntil: 5, archived: true))])
        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.userState == .init(mutedUntil: 5, archived: true))
    }

    @Test func anUnmutedAndUnarchivedRoomHasNoUserState() async throws {
        try await createRoom("lobby")
        try await fixture.context.saveDeclaration(Chat.RoomUserStateUpdate(
            chatId: fixture.chatId("lobby"),
            state: Chat.RoomUserState(mutedUntil: 5, archived: false)
        ))
        try await fixture.context.saveDeclaration(Chat.RoomUserStateUpdate(
            chatId: fixture.chatId("lobby"),
            state: .default
        ))

        #expect(try await fixture.context.roomUserStates(for: fixture.bot).isEmpty)
    }
}

private extension ChatRoomAdditionsStoreTests {
    var readAloud: Chat.RoomFeatures {
        Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, threadOf: nil, readAloud: true)
    }

    var reasoning: Chat.MessageMeta {
        Chat.MessageMeta(thread: nil, branch: nil, reasoning: Chat.Reasoning(durationMs: 4_000))
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

    func createRoom(_ roomId: String) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
    }

    func metaIndex(_ roomId: String) async throws -> Chat.MessageMetaIndex {
        try await ChatMessageMetaStore(databaseService: fixture.storageFacade.databaseService)
            .metaIndex(for: fixture.chatId(roomId))
    }

    func metaRowCount() async throws -> Int {
        try await fixture.storageFacade.databaseService.performRead { context in
            try context.count(for: CDChatMessageMeta.fetchRequest())
        }
    }
}

/// Gives a later time at each call, one second apart.
private final class TestClock: @unchecked Sendable {
    private var seconds: TimeInterval = 1_700_000_000
    private let lock = NSLock()

    var next: @Sendable () -> Date {
        { [self] in
            lock.withLock {
                seconds += 1
                return Date(timeIntervalSince1970: seconds)
            }
        }
    }
}
