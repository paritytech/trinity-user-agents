import Foundation
import Keystore_iOS
import Testing
import TrUAPIHost
@testable import polkadot_app

struct ProductVoIPCallsTests {
    private final class RecordingPublisher: ChatRoomEventPublishing, @unchecked Sendable {
        private let lock = NSLock()
        private var events: [(Chat.RoomEvent, String, String)] = []

        func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws {
            lock.withLock { events.append((event, productId, roomId)) }
        }

        var published: [(Chat.RoomEvent, String, String)] {
            lock.withLock { events }
        }
    }

    private static let key = Data(repeating: 7, count: 32)
    private static let pushId = String(repeating: "cd", count: 32)
    // A video call offer `call-1`, sealed by the product's TypeScript core under the key of 32 bytes of 7.
    private static let offer = "a118b624457bb32a6ac766909b5134cdbd41c8de1770c3a9af48b7ff11ba4136dc973f691944c8aecf3fde1d783e"
        + "37d623da70ff2758a6b8d169e7"
    // A text message from the same contact (see `ProductChatPushTests`).
    private static let text =
        "101f36f8c53c2d198a771dd74819c24e93548c5300f44446b1a9eb5aa17d89a71869be5057e20279b8df0a78155e57194fd14ec5b2c610d7c955f7059d7a45d5b3c7540dfe87"

    private func calls() throws -> ProductVoIPCalls {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        try routes.replaceChatRoutes(productId: "chat.paseo", routes: [
            Self.pushId: ProductPushRoute(productId: "chat.paseo", roomId: "dm-ab", key: Self.key, chatTitle: "alice.01")
        ])
        return ProductVoIPCalls(routes: routes)
    }

    @Test func aProductOfferRingsUnderTheContactsNameAndTheAnswerReachesItsRoomOnce() throws {
        let calls = try calls()
        let publisher = RecordingPublisher()

        let input = try #require(calls.input(pushId: Self.pushId.uppercased(), message: Self.offer))
        #expect(input.name == "alice.01")
        #expect(input.hasVideo)
        #expect(calls.ringingCall == ProductVoIPCall(productId: "chat.paseo", roomId: "dm-ab", callId: "call-1"))

        calls.answer(accept: true, publisher: publisher)
        calls.answer(accept: false, publisher: publisher)

        let published = publisher.published
        #expect(published.count == 1)
        #expect(published.first?.0 == .callAnswered(callId: "call-1", accept: true))
        #expect(published.first?.1 == "chat.paseo")
        #expect(published.first?.2 == "dm-ab")
    }

    @Test func aPushThatIsNoOfferOrHasNoRouteDoesNotRing() throws {
        let calls = try calls()

        #expect(calls.input(pushId: Self.pushId, message: Self.text) == nil)
        #expect(calls.input(pushId: String(repeating: "ab", count: 32), message: Self.offer) == nil)
        #expect(calls.ringingCall == nil)
    }
}
