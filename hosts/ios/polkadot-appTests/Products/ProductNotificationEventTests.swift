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
        #expect(
            event.hostEvent(
                step: .tapped,
                origin: GameReminderDiagnostics.testOrigin,
                monotonicMilliseconds: GameReminderDiagnostics.testMonotonicMilliseconds
            ) == HostEvent(
                name: "host.product_notification.tapped",
                tags: [
                    "host.app_variant": "test",
                    "host.platform": "ios",
                    "host.native_app_version": "1.2.3",
                    "host.os_version": "26.5",
                    "product_notification.product_id": "example.dot"
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
        #expect(
            ProductNotificationEvent(
                identifier: "product:localhost:3000:42",
                userInfo: [PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue],
                deliveredAt: deliveredAt
            )?.productId == "localhost:3000"
        )

        let firstLaunch = deliveredAt.addingTimeInterval(60 * 60)
        let secondLaunch = firstLaunch.addingTimeInterval(60 * 60)
        #expect(
            [
                ProductNotificationEvent.isNewSinceLastScan(
                    deliveredAt: deliveredAt,
                    scannedThrough: nil,
                    now: firstLaunch
                ),
                ProductNotificationEvent.isNewSinceLastScan(
                    deliveredAt: deliveredAt,
                    scannedThrough: firstLaunch,
                    now: secondLaunch
                ),
                ProductNotificationEvent.isNewSinceLastScan(
                    deliveredAt: deliveredAt,
                    scannedThrough: nil,
                    now: deliveredAt.addingTimeInterval(25 * 60 * 60)
                )
            ] == [true, false, false]
        )
    }

    @Test(arguments: [
        ("product:example.dot:42", PushNotificationSource.chat),
        ("chat:example.dot:42", .products),
        ("product::42", .products),
        ("product:example.dot", .products)
    ])
    func onlyNotificationsAProductScheduledAreReported(identifier: String, source: PushNotificationSource) {
        #expect(
            ProductNotificationEvent(
                identifier: identifier,
                userInfo: [PushNotificationKeys.pushSource: source.rawValue],
                deliveredAt: deliveredAt
            ) == nil
        )
    }
}
