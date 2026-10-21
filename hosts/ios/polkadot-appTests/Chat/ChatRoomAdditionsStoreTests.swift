import Foundation
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

    @Test func aRemovedRoomRemovesItsCall() async throws {
        try await createRoom("lobby")
        try await fixture.context.setRoomCall(
            for: fixture.bot,
            roomId: "lobby",
            call: .ringing(callId: "c1", video: false, caller: nil)
        )

        try await fixture.context.deleteRoom(for: fixture.bot, roomId: "lobby")

        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == nil)
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
        Chat.RoomFeatures(preimageAttachments: false, search: false, calls: nil, readAloud: true)
    }

    func createRoom(_ roomId: String) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
    }
}
