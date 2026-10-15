import Foundation
import Testing
@testable import polkadot_app

/// The calls of the product rooms in memory (07-LATER §11.2).
struct ChatRoomCallStoreTests {
    private let room = Chat.Id.chatExtension("game.dot", roomId: "lobby")

    @Test func aRingingCallGoesAfterTheRingingLifetime() async throws {
        let store = ChatRoomCallStore(ringingLifetime: .milliseconds(100))

        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        #expect(store.call(for: room) != nil)

        try await poll { store.call(for: room) == nil }
    }

    @Test func anActiveCallStaysAfterTheRingingLifetime() async throws {
        let store = ChatRoomCallStore(ringingLifetime: .milliseconds(50))

        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        store.set(.active(callId: "c1", video: false, startedAt: 1, mediaSession: nil), for: room)
        try await Task.sleep(for: .milliseconds(200))

        #expect(store.call(for: room) == .active(callId: "c1", video: false, startedAt: 1, mediaSession: nil))
    }

    /// The lifetime of a new ringing call starts again, so the end of an earlier one does not remove it.
    @Test func theEndOfAnEarlierRingingCallLeavesANewOne() async throws {
        let store = ChatRoomCallStore(ringingLifetime: .seconds(2))

        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        try await Task.sleep(for: .milliseconds(1_200))
        store.set(.ringing(callId: "c2", video: false, caller: nil), for: room)
        try await Task.sleep(for: .milliseconds(1_200))

        #expect(store.call(for: room) == .ringing(callId: "c2", video: false, caller: nil))
        try await poll { store.call(for: room) == nil }
    }

    @Test func changesGivesEachChangeOfEachRoom() async throws {
        let store = ChatRoomCallStore()
        let other = Chat.Id.chatExtension("game.dot", roomId: "other")
        var changes = store.changes().makeAsyncIterator()

        store.set(.ringing(callId: "c1", video: true, caller: "p1"), for: room)
        store.set(nil, for: other)

        #expect(await changes.next() == Chat.RoomCallChange(
            chatId: room,
            call: .ringing(callId: "c1", video: true, caller: "p1")
        ))
        #expect(await changes.next() == Chat.RoomCallChange(chatId: other, call: nil))
        #expect(store.allCalls().map(\.chatId) == [room])
    }

    @Test func clearRingingRemovesOnlyTheRingingCallsOfTheProduct() {
        let store = ChatRoomCallStore()
        let active = Chat.Id.chatExtension("game.dot", roomId: "active")
        let other = Chat.Id.chatExtension("other.dot", roomId: "lobby")
        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        store.set(.active(callId: "c2", video: false, startedAt: 1, mediaSession: nil), for: active)
        store.set(.ringing(callId: "c3", video: false, caller: nil), for: other)

        store.clearRinging(ofProduct: "game.dot")

        #expect(store.call(for: room) == nil)
        #expect(store.call(for: active) != nil)
        #expect(store.call(for: other) != nil)
    }
}

private extension ChatRoomCallStoreTests {
    func poll(_ condition: @escaping () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(10)
        while !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the condition did not become true in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
