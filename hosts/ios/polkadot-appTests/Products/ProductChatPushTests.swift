import Foundation
import Keystore_iOS
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The chat product gives the app one route per contact. The notification extension opens a push of that contact
/// with the key of the route, and shows it under the title of the route (`ProductPushHandler`).
struct ProductChatPushTests {
    private static let key = Data(repeating: 7, count: 32)

    @Test func aRouteKeepsTheRoomTheKeyAndTheTitleOfTheContact() async throws {
        let routes = ProductPushRoutes(keychain: InMemoryKeychain())
        let bridge = AppPushHostBridge(deviceToken: { nil }, routes: routes, relay: RecordingChatRelay())
        let pushId = String(repeating: "cd", count: 32)

        try await bridge.setChatPushRoutes(
            productId: "chat.paseo",
            routes: [HostChatPushRoute(pushId: pushId, roomId: "dm-ab", title: "alice.01", key: Self.key)]
        )

        let route = try #require(routes.route(pushId: pushId))
        #expect(route.productId == "chat.paseo")
        #expect(route.roomId == "dm-ab")
        #expect(route.key == Self.key)
        #expect(route.chatTitle == "alice.01")
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
}

private final class RecordingChatRelay: ProductPushRelaying, @unchecked Sendable {
    func notify(_: NotifyRequestParameters) async throws -> Bool {
        true
    }
}
