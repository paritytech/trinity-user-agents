import Foundation
import Keystore_iOS
import Testing
import TrUAPIHost
import UserNotifications
@testable import polkadot_app

/// The chat product gives the app one route per contact; a push from that contact then opens, with no worker, into a
/// notification with the native chat's text. The messages below were sealed by the product's TypeScript core
/// (`seal(key, encodeNotificationPayload(...))`) under the key of 32 bytes of 7.
struct ProductChatPushTests {
    private static let key = Data(repeating: 7, count: 32)
    private static let textMessage =
        "101f36f8c53c2d198a771dd74819c24e93548c5300f44446b1a9eb5aa17d89a71869be5057e20279b8df0a78155e57194fd14ec5b2c610d7c955f7059d7a45d5b3c7540dfe87"
    private static let reactedMessage =
        "b8cd55422013e4e0811e87d44c8e3b7563c4687fe9982b2dc3eb7b2d5ffbf51509a5cdfea256f36354d3b125d800f1adc07e"

    @Test func aChatPushOpensIntoTheTextOfTheMessageUnderTheContactsName() async throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let bridge = AppPushHostBridge(deviceToken: { nil }, routes: routes, relay: RecordingChatRelay())
        let pushId = String(repeating: "cd", count: 32)
        try await bridge.setChatPushRoutes(
            productId: "chat.paseo",
            routes: [HostChatPushRoute(pushId: pushId, roomId: "dm-ab", title: "alice.01", key: Self.key)]
        )

        let content = try #require(Self.content(pushId: pushId, message: Self.textMessage, routes: routes))
        let reaction = try #require(Self.content(pushId: pushId, message: Self.reactedMessage, routes: routes))

        #expect(content.title == "alice.01")
        #expect(content.body == "hello from the chat product")
        #expect(content.userInfo[PushNotificationKeys.deeplink] as? String == "polkadot://chat.paseo/-/chat/dm-ab")
        #expect(reaction.body == "reacted with 👍")
    }

    /// A contact the user removed must not keep showing notifications from a route the product no longer lists.
    @Test func aRouteThatIsNoLongerListedIsForgotten() async throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let bridge = AppPushHostBridge(deviceToken: { nil }, routes: routes, relay: RecordingChatRelay())
        let first = String(repeating: "01", count: 32)
        let second = String(repeating: "02", count: 32)
        try await bridge.setChatPushRoutes(productId: "chat.paseo", routes: [
            HostChatPushRoute(pushId: first, roomId: "dm-01", title: "a", key: Self.key),
            HostChatPushRoute(pushId: second, roomId: "dm-02", title: "b", key: Self.key)
        ])

        try await bridge.setChatPushRoutes(productId: "chat.paseo", routes: [
            HostChatPushRoute(pushId: second, roomId: "dm-02", title: "b", key: Self.key)
        ])

        #expect(routes.route(pushId: first) == nil)
        #expect(routes.route(pushId: second)?.chatTitle == "b")
    }

    @Test func aPushThatDoesNotOpenShowsTheFallback() throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let pushId = String(repeating: "cd", count: 32)
        try routes.replaceChatRoutes(productId: "chat.paseo", routes: [
            pushId: ProductPushRoute(productId: "chat.paseo", roomId: "dm-ab", key: Data(repeating: 8, count: 32), chatTitle: "alice.01")
        ])

        let content = try #require(Self.content(pushId: pushId, message: Self.textMessage, routes: routes))

        #expect(content.body == "New message")
    }

    private static func content(pushId: String, message: String, routes: ProductPushRoutes) -> UNNotificationContent? {
        ProductPushContent.make(
            userInfo: [PushNotificationKeys.pushId: pushId, PushNotificationKeys.message: message],
            original: UNNotificationContent(),
            fallback: ("Polkadot", "New message"),
            routes: routes
        )
    }
}

private final class RecordingChatRelay: ProductPushRelaying, @unchecked Sendable {
    func notify(_: NotifyRequestParameters) async throws -> Bool {
        true
    }
}
