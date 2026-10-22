import Foundation
import Testing
@testable import polkadot_app

/// The calls of the product rooms in memory (07-LATER §11.2).
struct ChatRoomCallStoreTests {
    private let room = Chat.Id.chatExtension("game.dot", roomId: "lobby")

    /// A ringing call has no lifetime of its own: it rings until the product clears it, as the native chat rings
    /// until the caller ends the call.
    @Test func aRingingCallStaysUntilTheProductClearsIt() async throws {
        let store = ChatRoomCallStore()
        var updates = store.callUpdates(for: room).makeAsyncIterator()
        #expect(await updates.next() == .some(nil))

        store.set(.ringing(callId: "c1", video: false, caller: "contact"), for: room)
        #expect(await updates.next() == .some(.ringing(callId: "c1", video: false, caller: "contact")))
        try await Task.sleep(for: .milliseconds(200))
        #expect(store.call(for: room) == .ringing(callId: "c1", video: false, caller: "contact"))

        store.set(nil, for: room)
        #expect(await updates.next() == .some(nil))
        #expect(store.call(for: room) == nil)
    }

    @Test func anAnsweredCallTurnsActive() {
        let store = ChatRoomCallStore()

        store.set(.ringing(callId: "c1", video: false, caller: nil), for: room)
        store.set(.active(callId: "c1", video: false, startedAt: 1), for: room)

        #expect(store.call(for: room) == .active(callId: "c1", video: false, startedAt: 1))
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
        store.set(.active(callId: "c2", video: false, startedAt: 1), for: active)
        store.set(.ringing(callId: "c3", video: false, caller: nil), for: other)

        store.clearRinging(ofProduct: "game.dot")

        #expect(store.call(for: room) == nil)
        #expect(store.call(for: active) != nil)
        #expect(store.call(for: other) != nil)
    }
}
