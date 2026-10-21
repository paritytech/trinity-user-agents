import Foundation
import Testing
@testable import polkadot_app

/// The user state of a product room goes to the store, then to the product as `UserStateChanged` (07-LATER §6.2).
struct ChatRoomUserStateRecorderTests {
    private let fixture = ProductRoomFixture()
    private let publisher = RecordingRoomEventPublisher()

    @Test func theStateIsStoredAndPublishedToTheProductOfTheRoom() async throws {
        try await createRoom()
        let state = Chat.RoomUserState(mutedUntil: 1_800_000_000_000, archived: true)

        try await recorder().record(state, for: fixture.chatId("lobby"))

        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.userState == state)
        #expect(publisher.events == [.init(event: .userStateChanged(state), productId: "game.dot", roomId: "lobby")])
    }

    /// The core reads the stored states when the worker subscribes, so the product gets the state then.
    @Test func aFailedPublishKeepsTheStoredState() async throws {
        try await createRoom()
        publisher.error = TestError.unavailable

        try await recorder().record(Chat.RoomUserState(mutedUntil: nil, archived: true), for: fixture.chatId("lobby"))

        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.userState.archived == true)
    }

    @Test func aChatThatIsNotAProductRoomHasNoUserState() async throws {
        await #expect(throws: ChatRoomUserStateError.notProductRoom) {
            try await recorder().record(.default, for: .chatExtension("game.dot", roomId: nil))
        }
        #expect(publisher.events.isEmpty)
    }
}

private extension ChatRoomUserStateRecorderTests {
    enum TestError: Error {
        case unavailable
    }

    func createRoom() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
    }

    func recorder() -> ChatRoomUserStateRecorder {
        ChatRoomUserStateRecorder(storageFacade: fixture.storageFacade, publisher: publisher, logger: StubLogger())
    }
}

/// Records the room events that the app sends to products.
final class RecordingRoomEventPublisher: ChatRoomEventPublishing, @unchecked Sendable {
    struct Published: Equatable {
        let event: Chat.RoomEvent
        let productId: String
        let roomId: String
    }

    var error: Error?
    private(set) var events: [Published] = []

    func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws {
        if let error { throw error }
        events.append(Published(event: event, productId: productId, roomId: roomId))
    }
}
