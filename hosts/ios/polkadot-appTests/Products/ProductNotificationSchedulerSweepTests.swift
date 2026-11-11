import struct TrUAPIHost.ProductId
import CoreData
import Foundation
import FoundationExt
import IssueMonitoring
import Operation_iOS
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
    private let storageFacade = StoreRefusingStorageFacade()

    private let scheduler: ProductNotificationScheduler

    init() {
        scheduler = ProductNotificationScheduler(
            storageFacade: storageFacade,
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

    private func notScheduled(_ id: UInt32?, reason: String) -> HostEvent {
        expectedProductNotificationHostEvent(
            "not_scheduled",
            productId: Self.product,
            id: id,
            level: .error,
            tags: ["product_notification.reason": reason]
        )
    }

    private func attemptedBookingId() throws -> UInt32 {
        let identifier = try #require(center.inner.attemptedRequests.last?.identifier)
        return try #require(identifier.split(separator: ":").last.flatMap { UInt32($0) })
    }

    private func pendingIdentifiers() async -> [String] {
        await center.inner.pendingNotificationRequests().map(\.identifier)
    }

    private func becomeActive() {
        applicationEvents.post(name: UIApplication.didBecomeActiveNotification, object: nil)
    }

    private func book() async throws -> UInt32 {
        try await scheduler.schedule(productId: productId, request: request)
    }

    private func bookInBackground() -> Task<UInt32, any Error> {
        Task { [scheduler, productId, request] in
            try await scheduler.schedule(productId: productId, request: request)
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
        _ = try await center.anotherCallArrives()
        center.release()
        let nextId = try await booking.value
        try await cancelling.value
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

        let cancelled = event("cancelled", id).name
        #expect(Array(recorder.events.prefix(2)) == [event("scheduled", id), event("scheduled", nextId)])
        #expect(recorder.events.dropFirst(2).map(\.name) == [cancelled, cancelled])
    }

    @Test func aRefusedStoreReadRecordsNotScheduledWithoutAnIdAndStillThrows() async throws {
        let id = try await book()
        storageFacade.refuseReads()

        await #expect(throws: ReadRefused.self) {
            _ = try await scheduler.schedule(productId: productId, request: request)
        }

        #expect(recorder.events == [event("scheduled", id), notScheduled(nil, reason: "sweep_failed")])
    }

    @Test func aRefusedStoreWriteLeavesNothingPending() async throws {
        storageFacade.refuseWrites()

        await #expect(throws: WriteRefused.self) {
            _ = try await book()
        }

        let id = try attemptedBookingId()
        #expect(await pendingIdentifiers() == [])
        #expect(recorder.events == [notScheduled(id, reason: "store_failed")])
    }

    @Test func aFailedPruneReportsTheStaleBookingOnlyOnTheRetry() async throws {
        let lostId = try await book()
        center.inner.losePending()
        storageFacade.refuseWrites()

        await #expect(throws: WriteRefused.self) {
            _ = try await book()
        }
        storageFacade.allowWrites()
        let nextId = try await book()

        #expect(
            recorder.events == [
                event("scheduled", lostId),
                notScheduled(nil, reason: "sweep_failed"),
                event("gone", lostId),
                event("scheduled", nextId)
            ]
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
        let withdrawnId = try attemptedBookingId()
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

    @Test func aCancelTakesTheBookingOffTheScreenWhenTheStoreFails() async throws {
        let id = try await book()
        storageFacade.refuseReads()

        await #expect(throws: ReadRefused.self) {
            try await scheduler.cancel(productId: productId, notificationId: id)
        }

        #expect(await pendingIdentifiers() == [])
    }

    @Test(arguments: [(UInt64(2_000_000_000_000), 2_000_000_000_000), (UInt64.max, Int.max)])
    func aBookingRecordsWhenItIsDue(requestedMs: UInt64, reportedMs: Int) async throws {
        let dated = ScheduledNotificationRequest(text: request.text, deeplink: nil, scheduledAtMs: requestedMs)

        let id = try await scheduler.schedule(productId: productId, request: dated)

        let undated = event("scheduled", id)
        let dueTime = ["product_notification.scheduled_at_unix_ms": HostEventValue.integer(reportedMs)]
        let expected = HostEvent(
            name: undated.name,
            tags: undated.tags,
            extras: undated.extras.merging(dueTime) { _, new in new }
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

private struct ReadRefused: Error {}

private struct WriteRefused: Error {}

/// Works like the test facade until told to refuse, then fails every read of a stored row or every write.
private final class StoreRefusingStorageFacade: UserDataStorageTestFacade, @unchecked Sendable {
    private let refusal = StoreRefusal()

    func refuseReads() {
        refusal.refusesReads = true
    }

    func refuseWrites() {
        refusal.refusesWrites = true
    }

    func allowWrites() {
        refusal.refusesWrites = false
    }

    override func createRepository<T, U>(
        filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor],
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U>
        where T: Operation_iOS.Identifiable, U: NSManagedObject {
        CoreDataRepository(
            databaseService: WriteRefusingDatabaseService(wrapped: databaseService, refusal: refusal),
            mapper: AnyCoreDataMapper(ReadRefusingMapper(wrapping: mapper, refusal: refusal)),
            filter: filter,
            sortDescriptors: sortDescriptors
        )
    }
}

private final class StoreRefusal: @unchecked Sendable {
    private let lock = NSLock()
    private var reads = false
    private var writes = false

    var refusesReads: Bool {
        get { lock.withLock { reads } }
        set { lock.withLock { reads = newValue } }
    }

    var refusesWrites: Bool {
        get { lock.withLock { writes } }
        set { lock.withLock { writes = newValue } }
    }
}

private final class ReadRefusingMapper<T: Operation_iOS.Identifiable, U: NSManagedObject>: CoreDataMapperProtocol {
    private let wrapped: AnyCoreDataMapper<T, U>
    private let refusal: StoreRefusal

    init(wrapping wrapped: AnyCoreDataMapper<T, U>, refusal: StoreRefusal) {
        self.wrapped = wrapped
        self.refusal = refusal
    }

    var entityIdentifierFieldName: String {
        wrapped.entityIdentifierFieldName
    }

    func transform(entity: U) throws -> T {
        guard !refusal.refusesReads else {
            throw ReadRefused()
        }
        return try wrapped.transform(entity: entity)
    }

    func populate(entity: U, from model: T, using context: NSManagedObjectContext) throws {
        try wrapped.populate(entity: entity, from: model, using: context)
    }
}

/// Fails writes at the service, since a deletion never reaches the mapper.
private struct WriteRefusingDatabaseService: CoreDataServiceProtocol {
    let wrapped: CoreDataServiceProtocol
    let refusal: StoreRefusal

    var configuration: CoreDataServiceConfigurationProtocol {
        wrapped.configuration
    }

    func performWrite<T>(_ block: @escaping CoreDataContextBlock<T>, completion: @escaping CoreDataResultBlock<T>) {
        guard !refusal.refusesWrites else {
            completion(.failure(WriteRefused()))
            return
        }
        wrapped.performWrite(block, completion: completion)
    }

    func performRead<T>(_ block: @escaping CoreDataContextBlock<T>, completion: @escaping CoreDataResultBlock<T>) {
        wrapped.performRead(block, completion: completion)
    }

    func performAsync(block: @escaping CoreDataContextInvocationBlock) {
        wrapped.performAsync(block: block)
    }

    func performObserve(block: @escaping CoreDataContextInvocationBlock) {
        wrapped.performObserve(block: block)
    }

    func performWithObserver(block: @escaping CoreDataWriterObserverBlock) {
        wrapped.performWithObserver(block: block)
    }

    func close() throws {
        try wrapped.close()
    }

    func drop() throws {
        try wrapped.drop()
    }
}
