import Foundation
import Testing
import UserNotifications
@testable import polkadot_app

/// A notification of a product that opens a room of the product shows as a person chat notification does (B7 of
/// the verbosity wasm product, parity rows G3 and G4): the room name is the title, and in the foreground the banner
/// shows only while neither that room nor the chat list is on screen.
struct ProductRoomNotificationTests {
    private let fixture = ProductRoomFixture()

    @Test func aRoomNotificationShowsTheRoomNameAsTitle() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "dm-7", name: "bob.42", icon: nil)
        let added = AddedRequests()
        let scheduler = ProductNotificationScheduler(
            storageFacade: fixture.storageFacade,
            notificationAdding: { added.append($0) },
            logger: StubLogger()
        )

        for deeplink in ["polkadot://game.dot/-/chat/dm-7", "polkadot://game.dot/-/chat/unknown", nil] {
            _ = try await scheduler.schedule(
                productId: "game.dot",
                request: .init(text: "hello", deeplink: deeplink, scheduledAtMs: nil)
            )
        }

        #expect(added.requests.map(\.content.title) == ["bob.42", "game.dot", "game.dot"])
        #expect(added.requests.map(\.content.body) == ["hello", "hello", "hello"])
        #expect(added.requests
            .allSatisfy { $0.content.userInfo[PushNotificationKeys.productId] as? String == "game.dot" })
    }

    /// A product does not name the room of another product, so a link into another product shows the product id.
    @Test func aLinkIntoAnotherProductKeepsTheProductIdAsTitle() async throws {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "dm-7", name: "bob.42", icon: nil)
        let added = AddedRequests()
        let scheduler = ProductNotificationScheduler(
            storageFacade: fixture.storageFacade,
            notificationAdding: { added.append($0) },
            logger: StubLogger()
        )

        _ = try await scheduler.schedule(
            productId: "other.dot",
            request: .init(text: "hello", deeplink: "polkadot://game.dot/-/chat/dm-7", scheduledAtMs: nil)
        )

        #expect(added.requests.map(\.content.title) == ["other.dot"])
    }

    @Test func theBannerShowsUnlessTheRoomOrTheChatListIsOnScreen() {
        let controller = PushForegroundPresentationController(logger: StubLogger())
        let content = productContent(productId: "game.dot", deeplink: "polkadot://game.dot/-/chat/dm-7")
        let screens: [(PushForegroundVisibleScreen, UNNotificationPresentationOptions)] = [
            (.other, [.banner]),
            (.chatExtension(extensionId: "game.dot", roomId: "dm-8"), [.banner]),
            (.chatExtension(extensionId: "other.dot", roomId: "dm-7"), [.banner]),
            (.chat(accountId: Data(repeating: 1, count: 32)), [.banner]),
            (.chatExtension(extensionId: "game.dot", roomId: "dm-7"), []),
            (.chatList, [])
        ]

        for (screen, expected) in screens {
            controller.updateVisibleScreen(screen)
            #expect(controller.presentationOptions(for: content) == expected, "\(screen)")
        }
    }

    /// A product notification with no room link is not a chat notification: it shows on every screen.
    @Test func aNotificationWithNoRoomLinkShowsOnEveryScreen() {
        let controller = PushForegroundPresentationController(logger: StubLogger())
        let contents = [
            productContent(productId: "game.dot", deeplink: nil),
            productContent(productId: "game.dot", deeplink: "polkadot://game.dot/settings"),
            productContent(productId: "other.dot", deeplink: "polkadot://game.dot/-/chat/dm-7")
        ]

        for screen in [PushForegroundVisibleScreen.chatList, .chatExtension(extensionId: "game.dot", roomId: "dm-7")] {
            controller.updateVisibleScreen(screen)
            #expect(contents.map { controller.presentationOptions(for: $0) } == [[.banner], [.banner], [.banner]])
        }
    }
}

private extension ProductRoomNotificationTests {
    func productContent(productId: String, deeplink: String?) -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        var userInfo: [String: Any] = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
            PushNotificationKeys.productId: productId
        ]
        if let deeplink {
            userInfo[PushNotificationKeys.deeplink] = deeplink
        }
        content.userInfo = userInfo
        return content
    }
}

private final class AddedRequests: @unchecked Sendable {
    private let lock = NSLock()
    private var added: [UNNotificationRequest] = []

    var requests: [UNNotificationRequest] {
        lock.withLock { added }
    }

    func append(_ request: UNNotificationRequest) {
        lock.withLock { added.append(request) }
    }
}
