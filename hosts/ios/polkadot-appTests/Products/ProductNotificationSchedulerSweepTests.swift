import struct TrUAPIHost.ProductId
import Foundation
import FoundationExt
import IssueMonitoring
import Products
import Testing
import UIKit
import UserNotifications
@testable import polkadot_app

@Suite("Product notification scheduler sweep")
struct ProductNotificationSchedulerSweepTests {
    private static let product = "example.dot"

    private let productId = try! ProductId(Self.product)
    private let request = ScheduledNotificationRequest(text: "Your game starts soon", deeplink: nil, scheduledAtMs: nil)
    private let firedAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let center = HoldingNotificationCenter()
    private let recorder = RecordingHostEventRecorder()
    private let applicationEvents = NotificationCenter()

    private let scheduler: ProductNotificationScheduler

    init() {
        scheduler = ProductNotificationScheduler(
            storageFacade: UserDataStorageTestFacade(),
            notificationCenter: center,
            diagnostics: ProductNotificationDiagnostics.recording(into: recorder),
            applicationStateStreams: ApplicationStateStreamFactory(notificationCenter: applicationEvents)
        )
    }

    private func event(_ step: String, _ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent(step, productId: Self.product, id: id)
    }

    private func delivered(_ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent("delivered", productId: Self.product, id: id, deliveredAt: firedAt)
    }

    private func pendingIdentifiers() async -> [String] {
        await center.inner.pendingNotificationRequests().map(\.identifier)
    }

    private func becomeActive() {
        applicationEvents.post(name: UIApplication.didBecomeActiveNotification, object: nil)
    }

    private func book() async throws -> UInt32 {
        try await scheduler.schedule(productId: productId, title: nil, request: request)
    }

    private func bookInBackground() -> Task<UInt32, any Error> {
        Task { [scheduler, productId, request] in
            try await scheduler.schedule(productId: productId, title: nil, request: request)
        }
    }

    /// Books with the sweep of that booking held at its pending read while `cancellation` runs.
    private func bookHeldAtItsSweep(
        during cancellation: @escaping @Sendable () async throws -> Void
    ) async throws -> UInt32 {
        center.holdNextPendingRead()
        let booking = bookInBackground()
        try await center.waitUntilHeld()
        let cancelling = Task { try await cancellation() }
        let overlapped = try await center.anotherCallArrives()
        center.release()
        let nextId = try await booking.value
        try await cancelling.value
        #expect(!overlapped)
        return nextId
    }

    @Test func startReportsABookingThatLeftWhileTheAppWasClosed() async throws {
        let id = try await book()
        center.inner.losePending()

        await scheduler.start()
        try await recorder.waitForEvents(count: 2, timeout: .seconds(5))

        #expect(recorder.events == [event("scheduled", id), event("gone", id)])
    }

    @Test func everyActivationAfterStartSweeps() async throws {
        let id = try await book()
        await scheduler.start()
        try await center.waitUntilPendingWasRead()
        center.inner.losePending()

        becomeActive()
        try await recorder.waitForEvents(count: 2, timeout: .seconds(5))

        #expect(recorder.events == [event("scheduled", id), event("gone", id)])
    }

    @Test func aBookingDuringASweepDoesNotReportAStaleBookingTwice() async throws {
        let lostId = try await book()
        center.inner.losePending()
        center.holdNextDeliveredRead()

        let firstBooking = bookInBackground()
        try await center.waitUntilHeld()
        let secondBooking = bookInBackground()
        _ = try await center.anotherCallArrives()
        center.release()
        let firstId = try await firstBooking.value
        let secondId = try await secondBooking.value

        #expect(
            recorder.events == [
                event("scheduled", lostId),
                event("gone", lostId),
                event("scheduled", firstId),
                event("scheduled", secondId)
            ]
        )
    }

    @Test func anActivationSweepWaitsForTheSweepOfABooking() async throws {
        let lostId = try await book()
        center.inner.losePending()
        center.holdNextDeliveredRead()

        let booking = bookInBackground()
        try await center.waitUntilHeld()
        await scheduler.start()
        let overlapped = try await center.anotherCallArrives()
        center.release()
        let nextId = try await booking.value

        #expect(!overlapped)
        #expect(recorder.events == [event("scheduled", lostId), event("gone", lostId), event("scheduled", nextId)])
    }

    @Test func aCancelDuringASweepIsNotAlsoReportedGone() async throws {
        let id = try await book()

        let nextId = try await bookHeldAtItsSweep { [scheduler, productId] in
            try await scheduler.cancel(productId: productId, notificationId: id)
        }

        #expect(recorder.events == [event("scheduled", id), event("scheduled", nextId), event("cancelled", id)])
    }

    @Test func aCancelAllDuringASweepIsNotAlsoReportedGone() async throws {
        let id = try await book()

        let nextId = try await bookHeldAtItsSweep { [scheduler, productId] in
            try await scheduler.cancelAll(forProductId: productId)
        }

        #expect(Array(recorder.events.prefix(2)) == [event("scheduled", id), event("scheduled", nextId)])
        #expect(
            sortedByBookingId(recorder.events.dropFirst(2))
                == sortedByBookingId([event("cancelled", id), event("cancelled", nextId)])
        )
    }

    @Test func aWithdrawnBookingLeavesNothingPending() async throws {
        let keptId = try await book()
        center.holdNextPendingRead()
        let withdrawn = bookInBackground()
        try await center.waitUntilHeld()

        withdrawn.cancel()
        center.release()

        await #expect(throws: CancellationError.self) {
            _ = try await withdrawn.value
        }
        let withdrawnId = try #require(center.inner.lastAttemptedBookingId)
        let kept = ScheduledNotificationEntry(productId: productId, notificationId: keptId)
        #expect(await pendingIdentifiers() == [kept.unIdentifier])
        #expect(
            recorder.events == [
                event("scheduled", keptId),
                event("scheduled", withdrawnId),
                event("cancelled", withdrawnId)
            ]
        )
    }

    @Test func aCancelOfAFiredBookingReportsTheDeliveryAndNoCancel() async throws {
        let id = try await book()
        center.inner.firePending(at: firedAt)

        try await scheduler.cancel(productId: productId, notificationId: id)

        #expect(recorder.events == [event("scheduled", id), delivered(id)])
    }

    @Test func aCancelAllOfAFiredBookingReportsTheDeliveryAndNoCancel() async throws {
        let id = try await book()
        center.inner.firePending(at: firedAt)

        try await scheduler.cancelAll(forProductId: productId)

        #expect(recorder.events == [event("scheduled", id), delivered(id)])
    }

    @Test func aCancelOfAnUnknownBookingRecordsNothing() async throws {
        try await scheduler.cancel(productId: productId, notificationId: 7)

        #expect(recorder.events == [])
    }

    @Test(arguments: [(UInt64(2_000_000_000_000), 2_000_000_000_000), (UInt64.max, Int.max)])
    func aBookingRecordsWhenItIsDue(requestedMs: UInt64, reportedMs: Int) async throws {
        let dated = ScheduledNotificationRequest(text: request.text, deeplink: nil, scheduledAtMs: requestedMs)

        let id = try await scheduler.schedule(productId: productId, title: nil, request: dated)

        let expected = expectedProductNotificationHostEvent(
            "scheduled",
            productId: Self.product,
            id: id,
            scheduledAtMs: reportedMs
        )
        #expect(recorder.events == [expected])
    }
}

/// Holds one read of the wrapped center open, so a test can run other work inside that suspension.
private final class HoldingNotificationCenter: ScheduledNotificationCentering, @unchecked Sendable {
    private enum Read {
        case pending
        case delivered
    }

    let inner = FakeScheduledNotificationCenter()

    private let lock = NSLock()
    private var heldRead: Read?
    private var hasArrived = false
    private var waitingRead: CheckedContinuation<Void, Never>?
    private var otherCallArrived = false
    private var pendingWasRead = false

    func holdNextPendingRead() {
        lock.withLock { heldRead = .pending }
    }

    func holdNextDeliveredRead() {
        lock.withLock { heldRead = .delivered }
    }

    func waitUntilHeld() async throws {
        try await waitUntil { hasArrived }
    }

    func waitUntilPendingWasRead() async throws {
        try await waitUntil { pendingWasRead }
    }

    /// Whether a call reaches the center during the hold. Only work that should have waited can, so a regression
    /// answers at once and correct code runs out the bound, which it passes however slow the machine is.
    func anotherCallArrives(within bound: Duration = .seconds(1)) async throws -> Bool {
        let deadline = ContinuousClock.now + bound
        while ContinuousClock.now < deadline {
            guard !lock.withLock({ otherCallArrived }) else {
                return true
            }
            try await Task.sleep(for: .milliseconds(5))
        }
        return false
    }

    func release() {
        lock.withLock {
            let read = waitingRead
            waitingRead = nil
            return read
        }?.resume()
    }

    func add(_ request: UNNotificationRequest) async throws {
        noteCall()
        try await inner.add(request)
    }

    func pendingNotificationRequests() async -> [UNNotificationRequest] {
        noteCall()
        await hold(.pending)
        let requests = await inner.pendingNotificationRequests()
        lock.withLock { pendingWasRead = true }
        return requests
    }

    func deliveredNotifications() async -> [UNNotification] {
        noteCall()
        await hold(.delivered)
        return await inner.deliveredNotifications()
    }

    func removePendingNotificationRequests(withIdentifiers identifiers: [String]) {
        inner.removePendingNotificationRequests(withIdentifiers: identifiers)
        noteCall()
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 500 where !lock.withLock(condition) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let conditionHolds = lock.withLock(condition)
        try #require(conditionHolds)
    }

    private func noteCall() {
        lock.withLock {
            if waitingRead != nil {
                otherCallArrived = true
            }
        }
    }

    private func hold(_ read: Read) async {
        guard lock.withLock({ heldRead == read }) else {
            return
        }
        await withCheckedContinuation { continuation in
            lock.withLock {
                heldRead = nil
                hasArrived = true
                waitingRead = continuation
            }
        }
    }
}
