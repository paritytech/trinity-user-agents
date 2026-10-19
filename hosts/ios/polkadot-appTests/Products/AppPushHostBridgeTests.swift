import CryptoKit
import Foundation
import Keystore_iOS
import Testing
import TrUAPIHost
import UserNotifications
@testable import polkadot_app

/// A product room gets a push route (a push id and a key) that the notification service extension reads, and a
/// product push opens with that key into a notification that opens the room.
struct AppPushHostBridgeTests {
    private let token = Data(repeating: 0xAB, count: 32)

    @Test func eachRoomKeepsItsRouteAndNeedsAPushToken() async throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let bridge = AppPushHostBridge(deviceToken: { [token] in token }, routes: routes, relay: RecordingRelay())

        let first = try #require(try await bridge.pushRoute(productId: "verbosity.dot", roomId: "dm-7"))
        let again = try #require(try await bridge.pushRoute(productId: "verbosity.dot", roomId: "dm-7"))
        let other = try #require(try await bridge.pushRoute(productId: "verbosity.dot", roomId: "dm-8"))

        #expect(first == again)
        #expect(first.pushId != other.pushId)
        #expect(first.pushId.count == 32)
        #expect(first.key.count == 32)
        #expect(first.token == token.map { String(format: "%02x", $0) }.joined())
        #expect(routes.route(pushId: first.pushId) == ProductPushRoute(
            productId: "verbosity.dot",
            roomId: "dm-7",
            key: first.key
        ))

        let noToken = AppPushHostBridge(deviceToken: { nil }, routes: routes, relay: RecordingRelay())
        #expect(try await noToken.pushRoute(productId: "verbosity.dot", roomId: "dm-7") == nil)
    }

    @Test func aDeliveryGoesToTheRelayAsTheNativeChatSendsIt() async throws {
        let relay = RecordingRelay()
        let bridge = AppPushHostBridge(deviceToken: { nil }, relay: relay)

        let taken = try await bridge.sendPush(
            productId: "verbosity.dot",
            delivery: HostPushDelivery(
                service: .apns,
                token: "abcd",
                pushId: String(repeating: "1", count: 32),
                message: "00ff"
            )
        )

        #expect(taken)
        let sent = try #require(relay.sent.first)
        #expect(sent.deviceToken == "abcd")
        #expect(sent.platform == "ios")
        #expect(sent.message == "00ff")
        #expect(!sent.voip)
    }

    @Test func aProductPushOpensWithTheKeyOfItsRoomIntoALinkToTheRoom() throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let key = Data(repeating: 9, count: 32)
        let pushId = String(repeating: "a", count: 32)
        try routes.save(ProductPushRoute(productId: "verbosity.dot", roomId: "dm-7", key: key), pushId: pushId)
        let message = try Self.seal(#"{"body":"hello","title":"@alice.42","v":1}"#, key: key)

        let content = try #require(ProductPushContent.make(
            userInfo: [PushNotificationKeys.pushId: pushId, PushNotificationKeys.message: message],
            original: UNNotificationContent(),
            fallback: ("Polkadot", "New message"),
            routes: routes
        ))

        #expect(content.title == "@alice.42")
        #expect(content.body == "hello")
        #expect(content.userInfo[PushNotificationKeys.deeplink] as? String == "polkadot://verbosity.dot/-/chat/dm-7")
        #expect(content.userInfo[PushNotificationKeys.productId] as? String == "verbosity.dot")
        #expect(ProductPushContent.make(
            userInfo: [
                PushNotificationKeys.pushId: String(repeating: "b", count: 32),
                PushNotificationKeys.message: message
            ],
            original: UNNotificationContent(),
            fallback: ("Polkadot", "New message"),
            routes: routes
        ) == nil)
        #expect(ProductPushNotification.open(messageHex: message, key: Data(repeating: 1, count: 32)) == nil)
    }

    /// XChaCha20-Poly1305 of `json` under `key`, as the core of the sending device makes it.
    private static func seal(_ json: String, key: Data) throws -> String {
        let nonce = Data(repeating: 3, count: 24)
        let subkey = HChaCha20.subkey(key: key, nonce: nonce.prefix(16))
        let box = try ChaChaPoly.seal(
            Data(json.utf8),
            using: SymmetricKey(data: subkey),
            nonce: ChaChaPoly.Nonce(data: Data(count: 4) + nonce.suffix(8)),
            authenticating: ProductPushNotification.associatedData
        )
        return (nonce + box.ciphertext + box.tag).map { String(format: "%02x", $0) }.joined()
    }
}

private final class RecordingRelay: ProductPushRelaying, @unchecked Sendable {
    private(set) var sent: [NotifyRequestParameters] = []

    func notify(_ parameters: NotifyRequestParameters) async throws -> Bool {
        sent.append(parameters)
        return true
    }
}
