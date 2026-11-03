import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

struct ProductNotificationEventTests {
    private let deliveredAt = Date(timeIntervalSince1970: 2_000_000_000)

    @Test func productNotificationNamesTheProductAndDeeplinkButNotItsText() throws {
        let event = try #require(
            ProductNotificationEvent(
                identifier: "product:example.dot:42",
                userInfo: [
                    PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
                    PushNotificationKeys.deeplink: "example.dot/game/7"
                ],
                deliveredAt: deliveredAt
            )
        )

        #expect(
            event.hostEvent(
                step: .shown,
                seenVia: .nextLaunch,
                origin: GameReminderDiagnostics.testOrigin,
                monotonicMilliseconds: GameReminderDiagnostics.testMonotonicMilliseconds
            ) == HostEvent(
                name: "host.product_notification.shown",
                tags: [
                    "host.app_variant": "test",
                    "host.platform": "ios",
                    "host.native_app_version": "1.2.3",
                    "host.os_version": "26.5",
                    "product_notification.product_id": "example.dot",
                    "product_notification.seen_via": "next_launch"
                ],
                extras: [
                    "host.app_build": .integer(456),
                    "event.monotonic_ms": .integer(7),
                    "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"),
                    "product_notification.deeplink": .string("example.dot/game/7"),
                    "product_notification.delivered_at_unix_ms": .integer(2_000_000_000_000)
                ]
            )
        )
    }

    @Test func chatNotificationIsNotReported() {
        #expect(
            ProductNotificationEvent(
                identifier: "product:example.dot:42",
                userInfo: [PushNotificationKeys.pushSource: PushNotificationSource.chat.rawValue],
                deliveredAt: deliveredAt
            ) == nil
        )
    }
}
