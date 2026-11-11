import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

@Suite("Product notification diagnostics")
struct ProductNotificationDiagnosticsTests {
    private let deliveredAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let recorder = RecordingHostEventRecorder()

    private var diagnostics: ProductNotificationDiagnostics {
        .recording(into: recorder)
    }

    private let productId = try! ProductId("example.dot")

    private var booking: ScheduledNotificationEntry {
        ScheduledNotificationEntry(productId: productId, notificationId: 42)
    }

    private func productInfo(deeplink: String? = nil) -> [AnyHashable: Any] {
        var info: [AnyHashable: Any] = [PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue]
        info[PushNotificationKeys.deeplink] = deeplink
        return info
    }

    private func expectedEvent(
        _ step: String,
        productId: String,
        id: UInt32? = 42,
        deliveredAt: Date? = nil,
        level: HostLogLevel = .info,
        tags: [String: String] = [:]
    ) -> HostEvent {
        expectedProductNotificationHostEvent(
            step,
            productId: productId,
            id: id,
            deliveredAt: deliveredAt,
            level: level,
            tags: tags
        )
    }

    @Test func shownThenTappedNamesOnlyTheProductNotItsDeeplinkOrText() {
        let info = productInfo(deeplink: "example.dot/game/7")
        diagnostics.shown(identifier: "product:example.dot:42", userInfo: info, deliveredAt: deliveredAt)
        diagnostics.tapped(identifier: "product:example.dot:42", userInfo: info, deliveredAt: deliveredAt)

        #expect(
            recorder.events == [
                expectedEvent("shown", productId: "example.dot", deliveredAt: deliveredAt),
                expectedEvent("tapped", productId: "example.dot", deliveredAt: deliveredAt)
            ]
        )
    }

    @Test(arguments: [
        ("product:example.dot:42", PushNotificationSource.chat),
        ("chat:example.dot:42", .products),
        ("product::42", .products),
        ("product:example.dot", .products),
        ("product:example.dot:not-a-number", .products)
    ])
    func notificationsAProductDidNotScheduleRecordNothing(identifier: String, source: PushNotificationSource) {
        let info: [AnyHashable: Any] = [PushNotificationKeys.pushSource: source.rawValue]
        diagnostics.shown(identifier: identifier, userInfo: info, deliveredAt: deliveredAt)
        diagnostics.suppressed(identifier: identifier, userInfo: info, deliveredAt: deliveredAt)
        diagnostics.tapped(identifier: identifier, userInfo: info, deliveredAt: deliveredAt)

        #expect(recorder.events == [])
    }

    @Test func aProductIdWithAPortKeepsIt() {
        diagnostics.shown(identifier: "product:localhost:3000:42", userInfo: productInfo(), deliveredAt: deliveredAt)

        #expect(recorder.events == [expectedEvent("shown", productId: "localhost:3000", deliveredAt: deliveredAt)])
    }

    @Test func scheduledRecordsProductAndBookingId() {
        diagnostics.scheduled(entry: booking)

        #expect(recorder.events == [expectedEvent("scheduled", productId: "example.dot")])
    }

    @Test func limitReachedCarriesNoBookingId() {
        diagnostics.notScheduled(productId: productId, notificationId: nil, reason: .limitReached)

        #expect(
            recorder.events == [
                expectedEvent(
                    "not_scheduled",
                    productId: "example.dot",
                    id: nil,
                    tags: ["product_notification.reason": "limit_reached"]
                )
            ]
        )
    }

    @Test(arguments: [
        (ProductNotificationEvent.Reason.sweepFailed, "sweep_failed"),
        (.readFailed, "read_failed"),
        (.addFailed, "add_failed"),
        (.storeFailed, "store_failed")
    ])
    func aFailedHandOverIsAnErrorCarryingTheBookingIdAndReason(reason: ProductNotificationEvent.Reason, tag: String) {
        diagnostics.notScheduled(productId: productId, notificationId: 42, reason: reason)

        #expect(
            recorder.events == [
                expectedEvent(
                    "not_scheduled",
                    productId: "example.dot",
                    level: .error,
                    tags: ["product_notification.reason": tag]
                )
            ]
        )
    }

    @Test(arguments: [
        (ProductNotificationEvent.Reason.sweepFailed, "sweep-failed"),
        (.readFailed, "read-failed"),
        (.addFailed, "add-failed"),
        (.storeFailed, "store-failed")
    ])
    func aFailedHandOverReportsAnIssueWithItsError(reason: ProductNotificationEvent.Reason, kind: String) {
        struct StoreError: Error {}
        let issues = RecordingIssueReporter()
        let diagnostics = ProductNotificationDiagnostics.recording(into: recorder, issues: issues)

        diagnostics.notScheduled(productId: productId, notificationId: 42, reason: reason, error: StoreError())

        #expect(issues.issues.map { [$0.flow, $0.kind, $0.errorType] } == [
            ["product-notification", kind, String(reflecting: StoreError.self)]
        ])
    }

    @Test func limitReachedReportsNoIssue() {
        let issues = RecordingIssueReporter()
        let diagnostics = ProductNotificationDiagnostics.recording(into: recorder, issues: issues)

        diagnostics.notScheduled(productId: productId, notificationId: nil, reason: .limitReached)

        #expect(issues.issues.isEmpty)
    }

    @Test func suppressedIsToldFromShown() {
        diagnostics.suppressed(identifier: "product:example.dot:42", userInfo: productInfo(), deliveredAt: deliveredAt)

        #expect(recorder.events == [expectedEvent("suppressed", productId: "example.dot", deliveredAt: deliveredAt)])
    }

    @Test func cancelledNamesTheBooking() {
        diagnostics.cancelled(entry: booking)

        #expect(recorder.events == [expectedEvent("cancelled", productId: "example.dot")])
    }

    @Test func deliveredCarriesTheTimeItFired() {
        diagnostics.delivered(entry: booking, deliveredAt: deliveredAt)

        #expect(recorder.events == [expectedEvent("delivered", productId: "example.dot", deliveredAt: deliveredAt)])
    }

    @Test func goneCarriesNoDeliveryTime() {
        diagnostics.gone(entry: booking)

        #expect(recorder.events == [expectedEvent("gone", productId: "example.dot")])
    }
}
