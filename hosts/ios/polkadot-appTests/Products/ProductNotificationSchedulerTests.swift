import struct TrUAPIHost.ProductId
import Foundation
import FoundationExt
import IssueMonitoring
import Products
import Testing
@testable import polkadot_app

@Suite("Product notification scheduler")
struct ProductNotificationSchedulerTests {
    private static let product = "example.dot"
    private static let slotLimit = 64

    private let productId = try! ProductId(Self.product)
    private let request = ScheduledNotificationRequest(
        text: "Your game starts soon",
        deeplink: "example.dot/game/7",
        scheduledAtMs: nil
    )
    private let firedAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let center = FakeScheduledNotificationCenter()
    private let recorder = RecordingHostEventRecorder()
    private let issues = RecordingIssueReporter()
    private let applicationEvents = NotificationCenter()

    private let scheduler: ProductNotificationScheduler

    init() {
        scheduler = ProductNotificationScheduler(
            storageFacade: UserDataStorageTestFacade(),
            notificationCenter: center,
            diagnostics: ProductNotificationDiagnostics.recording(into: recorder, issues: issues),
            applicationStateStreams: ApplicationStateStreamFactory(notificationCenter: applicationEvents)
        )
    }

    private func scheduled(_ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent("scheduled", productId: Self.product, id: id)
    }

    private func cancelled(_ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent("cancelled", productId: Self.product, id: id)
    }

    private func gone(_ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent("gone", productId: Self.product, id: id)
    }

    private func notScheduled(_ id: UInt32?, reason: String, level: HostLogLevel = .error) -> HostEvent {
        expectedProductNotificationHostEvent(
            "not_scheduled",
            productId: Self.product,
            id: id,
            level: level,
            tags: ["product_notification.reason": reason]
        )
    }

    @Test func bookingRecordsScheduled() async throws {
        let id = try await scheduler.schedule(productId: productId, title: nil, request: request)

        #expect(recorder.events == [scheduled(id)])
    }

    @Test func refusedHandOverRecordsNotScheduledAndStillThrows() async throws {
        center.refuseAdd()

        await #expect(throws: FakeScheduledNotificationCenter.AddRefused()) {
            _ = try await scheduler.schedule(productId: productId, title: nil, request: request)
        }

        let id = try #require(center.lastAttemptedBookingId)
        #expect(recorder.events == [notScheduled(id, reason: "add_failed")])
    }

    @Test func refusedHandOverReportsAnIssueWithTheRefusal() async throws {
        center.refuseAdd()

        await #expect(throws: FakeScheduledNotificationCenter.AddRefused()) {
            _ = try await scheduler.schedule(productId: productId, title: nil, request: request)
        }

        #expect(issues.issues.map { [$0.flow, $0.kind, $0.errorType] } == [
            ["product-notification", "add-failed", String(reflecting: FakeScheduledNotificationCenter.AddRefused.self)]
        ])
    }

    @Test func bookingPastTheSlotLimitRecordsNotScheduledWithoutAnId() async throws {
        var ids: [UInt32] = []
        for _ in 0 ..< Self.slotLimit {
            let id = try await scheduler.schedule(productId: productId, title: nil, request: request)
            ids.append(id)
        }

        let error = await #expect(throws: ProductNativeApiError.self) {
            _ = try await scheduler.schedule(productId: productId, title: nil, request: request)
        }

        #expect(error?.localizedDescription == ProductNativeApiError.scheduleLimitReached.localizedDescription)
        #expect(recorder.events == ids.map(scheduled) + [notScheduled(nil, reason: "limit_reached", level: .info)])
    }

    @Test func cancelRecordsCancelled() async throws {
        let id = try await scheduler.schedule(productId: productId, title: nil, request: request)

        try await scheduler.cancel(productId: productId, notificationId: id)

        #expect(recorder.events == [scheduled(id), cancelled(id)])
    }

    @Test func cancelAllRecordsOneCancelledPerBooking() async throws {
        let first = try await scheduler.schedule(productId: productId, title: nil, request: request)
        let second = try await scheduler.schedule(productId: productId, title: nil, request: request)

        try await scheduler.cancelAll(forProductId: productId)

        #expect(await center.pendingNotificationRequests().isEmpty)
        #expect(Array(recorder.events.prefix(2)) == [scheduled(first), scheduled(second)])
        #expect(
            sortedByBookingId(recorder.events.dropFirst(2)) == sortedByBookingId([cancelled(first), cancelled(second)])
        )
    }

    @Test func aCancelledBookingIsNotAlsoReportedGone() async throws {
        let cancelledId = try await scheduler.schedule(productId: productId, title: nil, request: request)
        try await scheduler.cancel(productId: productId, notificationId: cancelledId)

        let nextId = try await scheduler.schedule(productId: productId, title: nil, request: request)

        #expect(recorder.events == [scheduled(cancelledId), cancelled(cancelledId), scheduled(nextId)])
    }

    @Test func startReportsABookingStillInNotificationCenterAsDelivered() async throws {
        let id = try await scheduler.schedule(productId: productId, title: nil, request: request)
        center.firePending(at: firedAt)

        await scheduler.start()
        try await recorder.waitForEvents(count: 2)

        #expect(
            recorder.events == [
                scheduled(id),
                expectedProductNotificationHostEvent("delivered", productId: Self.product, id: id, deliveredAt: firedAt)
            ]
        )
    }

    @Test func theNextBookingReportsAStaleBookingOnce() async throws {
        let lostId = try await scheduler.schedule(productId: productId, title: nil, request: request)
        center.losePending()

        let secondId = try await scheduler.schedule(productId: productId, title: nil, request: request)
        let thirdId = try await scheduler.schedule(productId: productId, title: nil, request: request)

        #expect(recorder.events == [scheduled(lostId), gone(lostId), scheduled(secondId), scheduled(thirdId)])
    }

    @Test func aBookingStillPendingIsNotReported() async throws {
        let firstId = try await scheduler.schedule(productId: productId, title: nil, request: request)

        let secondId = try await scheduler.schedule(productId: productId, title: nil, request: request)

        #expect(recorder.events == [scheduled(firstId), scheduled(secondId)])
    }
}
