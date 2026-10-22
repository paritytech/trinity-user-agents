import Foundation
import Testing
@testable import polkadot_app

/// The chat store keeps what a product sets through the Chat Extension v2 additions and the contact of a room
/// (07-LATER §4.4, §9.6).
struct ChatRoomAdditionsStoreTests {
    private let fixture = ProductRoomFixture()

    @Test func theFeaturesReachThePolicyOfTheRoom() async throws {
        try await createRoom("lobby")

        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: calls)

        #expect(try await fixture.context.roomFeatures(for: fixture.bot, roomId: "lobby") == calls)
        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.features == calls)
    }

    @Test func theFeaturesLeaveTheOtherDeclarationsAlone() async throws {
        try await createRoom("lobby")
        let preview = Chat.RoomPreview(timestamp: 9, badge: .none, lastMessage: nil, unseenReaction: true)
        try await fixture.context.setRoomPreview(for: fixture.bot, roomId: "lobby", preview: preview)

        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: calls)

        let declarations = try await fixture.context.roomDeclarations(for: fixture.bot, roomId: "lobby")
        #expect(declarations.preview == preview)
        #expect(declarations.features == calls)
    }

    @Test func theContactReachesThePolicyAndLeavesTheFeaturesAlone() async throws {
        try await createRoom("lobby")
        try await fixture.context.setRoomFeatures(for: fixture.bot, roomId: "lobby", features: calls)
        let contact = ProductRoomTestData.contact(.incoming)

        try await fixture.context.setRoomContact(for: fixture.bot, roomId: "lobby", contact: contact)

        let chat = try #require(try await fixture.storedChat("lobby"))
        #expect(chat.roomContact == contact)
        #expect(chat.roomPolicy?.features == calls)
    }

    @Test func aContactForAnUnknownRoomIsRefused() async throws {
        await #expect(throws: ChatRoomError.self) {
            try await fixture.context.setRoomContact(
                for: fixture.bot,
                roomId: "missing",
                contact: ProductRoomTestData.contact(.established)
            )
        }
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
}

private extension ChatRoomAdditionsStoreTests {
    var calls: Chat.RoomFeatures {
        Chat.RoomFeatures(calls: Chat.RoomCalls(audio: true, video: true), hopAttachments: false)
    }

    func createRoom(_ roomId: String) async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
    }
}
