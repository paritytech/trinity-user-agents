import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

@Suite("Product notification diagnostics")
struct ProductNotificationDiagnosticsTests {
    private let deliveredAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let recorder = RecordingHostEventRecorder()

    private var diagnostics: ProductNotificationDiagnostics {
        ProductNotificationDiagnostics(
            recorder: recorder,
            origin: GameReminderDiagnostics.testOrigin,
            monotonicMilliseconds: { GameReminderDiagnostics.testMonotonicMilliseconds }
        )
    }

    private func productInfo(deeplink: String? = nil) -> [AnyHashable: Any] {
        var info: [AnyHashable: Any] = [PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue]
        info[PushNotificationKeys.deeplink] = deeplink
        return info
    }

    private func expectedEvent(_ step: String, productId: String) -> HostEvent {
        HostEvent(
            name: "host.product_notification.\(step)",
            tags: [
                "host.app_variant": "test",
                "host.platform": "ios",
                "host.native_app_version": "1.2.3",
                "host.os_version": "26.5",
                "product_notification.product_id": productId
            ],
            extras: [
                "host.app_build": .integer(456),
                "event.monotonic_ms": .integer(GameReminderDiagnostics.testMonotonicMilliseconds),
                "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"),
                "product_notification.delivered_at_unix_ms": .integer(2_000_000_000_000)
            ]
        )
    }

    @Test func shownThenTappedNamesOnlyTheProductNotItsDeeplinkOrText() {
        let info = productInfo(deeplink: "example.dot/game/7")
        diagnostics.shown(identifier: "product:example.dot:42", userInfo: info, deliveredAt: deliveredAt)
        diagnostics.tapped(identifier: "product:example.dot:42", userInfo: info, deliveredAt: deliveredAt)

        #expect(
            recorder.events == [
                expectedEvent("shown", productId: "example.dot"),
                expectedEvent("tapped", productId: "example.dot")
            ]
        )
    }

    @Test(arguments: [
        ("product:example.dot:42", PushNotificationSource.chat),
        ("chat:example.dot:42", .products),
        ("product::42", .products),
        ("product:example.dot", .products)
    ])
    func notificationsAProductDidNotScheduleRecordNothing(identifier: String, source: PushNotificationSource) {
        let info: [AnyHashable: Any] = [PushNotificationKeys.pushSource: source.rawValue]
        diagnostics.shown(identifier: identifier, userInfo: info, deliveredAt: deliveredAt)
        diagnostics.tapped(identifier: identifier, userInfo: info, deliveredAt: deliveredAt)

        #expect(recorder.events == [])
    }

    @Test func aProductIdWithAPortKeepsIt() {
        diagnostics.shown(identifier: "product:localhost:3000:42", userInfo: productInfo(), deliveredAt: deliveredAt)

        #expect(recorder.events == [expectedEvent("shown", productId: "localhost:3000")])
    }
}
