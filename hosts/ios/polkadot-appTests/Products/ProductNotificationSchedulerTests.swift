import struct TrUAPIHost.ProductId
import CoreData
import Foundation
import FoundationExt
import IssueMonitoring
import Operation_iOS
import Products
import Testing
import UIKit
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

    private func attemptedBookingId() throws -> UInt32 {
        let identifier = try #require(center.attemptedRequests.last?.identifier)
        return try #require(identifier.split(separator: ":").last.flatMap { UInt32($0) })
    }

    private func becomeActive() {
        applicationEvents.post(name: UIApplication.didBecomeActiveNotification, object: nil)
    }

    @Test func bookingRecordsScheduled() async throws {
        let id = try await scheduler.schedule(productId: productId, request: request)

        #expect(recorder.events == [scheduled(id)])
    }

    @Test func refusedHandOverRecordsNotScheduledAndStillThrows() async throws {
        center.refuseAdd()

        await #expect(throws: FakeScheduledNotificationCenter.AddRefused()) {
            _ = try await scheduler.schedule(productId: productId, request: request)
        }

        let id = try attemptedBookingId()
        #expect(recorder.events == [notScheduled(id, reason: "add_failed")])
    }

    @Test func refusedStoreWriteRecordsNotScheduledAndStillThrows() async throws {
        let scheduler = ProductNotificationScheduler(
            storageFacade: SaveRefusingStorageFacade(),
            notificationCenter: center,
            diagnostics: ProductNotificationDiagnostics.recording(into: recorder),
            applicationStateStreams: ApplicationStateStreamFactory(notificationCenter: applicationEvents)
        )

        await #expect(throws: SaveRefused.self) {
            _ = try await scheduler.schedule(productId: productId, request: request)
        }

        let id = try attemptedBookingId()
        #expect(recorder.events == [notScheduled(id, reason: "store_failed")])
    }

    @Test func bookingPastTheSlotLimitRecordsNotScheduledWithoutAnId() async throws {
        var ids: [UInt32] = []
        for _ in 0 ..< Self.slotLimit {
            let id = try await scheduler.schedule(productId: productId, request: request)
            ids.append(id)
        }

        let error = await #expect(throws: ProductNativeApiError.self) {
            _ = try await scheduler.schedule(productId: productId, request: request)
        }

        #expect(error?.localizedDescription == ProductNativeApiError.scheduleLimitReached.localizedDescription)
        #expect(recorder.events == ids.map(scheduled) + [notScheduled(nil, reason: "limit_reached", level: .info)])
    }

    @Test func cancelRecordsCancelled() async throws {
        let id = try await scheduler.schedule(productId: productId, request: request)

        try await scheduler.cancel(productId: productId, notificationId: id)

        #expect(recorder.events == [scheduled(id), cancelled(id)])
    }

    @Test func cancelAllRecordsOneCancelledPerBooking() async throws {
        let first = try await scheduler.schedule(productId: productId, request: request)
        let second = try await scheduler.schedule(productId: productId, request: request)

        try await scheduler.cancelAll(forProductId: productId)

        let byId: (HostEvent, HostEvent) -> Bool = {
            $0.tags["product_notification.id", default: ""] < $1.tags["product_notification.id", default: ""]
        }
        let cancellations = [cancelled(first), cancelled(second)].sorted(by: byId)
        #expect(Array(recorder.events.prefix(2)) == [scheduled(first), scheduled(second)])
        #expect(recorder.events.dropFirst(2).sorted(by: byId) == cancellations)
    }

    @Test func aCancelledBookingIsNotAlsoReportedGone() async throws {
        let cancelledId = try await scheduler.schedule(productId: productId, request: request)
        try await scheduler.cancel(productId: productId, notificationId: cancelledId)

        let nextId = try await scheduler.schedule(productId: productId, request: request)

        #expect(recorder.events == [scheduled(cancelledId), cancelled(cancelledId), scheduled(nextId)])
    }

    @Test func activationReportsABookingNoLongerPendingOrDeliveredAsGone() async throws {
        let id = try await scheduler.schedule(productId: productId, request: request)
        center.losePending()
        await scheduler.start()

        becomeActive()
        try await recorder.waitForEvents(count: 2)

        #expect(recorder.events == [scheduled(id), gone(id)])
    }

    @Test func activationReportsABookingStillInNotificationCenterAsDelivered() async throws {
        let id = try await scheduler.schedule(productId: productId, request: request)
        center.firePending(at: firedAt)
        await scheduler.start()

        becomeActive()
        try await recorder.waitForEvents(count: 2)

        #expect(
            recorder.events == [
                scheduled(id),
                expectedProductNotificationHostEvent("delivered", productId: Self.product, id: id, deliveredAt: firedAt)
            ]
        )
    }

    @Test func theNextBookingReportsAStaleBookingOnce() async throws {
        let lostId = try await scheduler.schedule(productId: productId, request: request)
        center.losePending()

        let secondId = try await scheduler.schedule(productId: productId, request: request)
        let thirdId = try await scheduler.schedule(productId: productId, request: request)

        #expect(recorder.events == [scheduled(lostId), gone(lostId), scheduled(secondId), scheduled(thirdId)])
    }

    @Test func aBookingStillPendingIsNotReported() async throws {
        let firstId = try await scheduler.schedule(productId: productId, request: request)

        let secondId = try await scheduler.schedule(productId: productId, request: request)

        #expect(recorder.events == [scheduled(firstId), scheduled(secondId)])
    }
}

private struct SaveRefused: Error {}

/// Reads like the test facade, and fails every write of a model.
private final class SaveRefusingStorageFacade: UserDataStorageTestFacade {
    override func createRepository<T, U>(
        filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor],
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U>
        where T: Operation_iOS.Identifiable, U: NSManagedObject {
        super.createRepository(
            filter: filter,
            sortDescriptors: sortDescriptors,
            mapper: AnyCoreDataMapper(SaveRefusingMapper(reading: mapper))
        )
    }
}

private final class SaveRefusingMapper<T: Operation_iOS.Identifiable, U: NSManagedObject>: CoreDataMapperProtocol {
    private let reading: AnyCoreDataMapper<T, U>

    init(reading: AnyCoreDataMapper<T, U>) {
        self.reading = reading
    }

    var entityIdentifierFieldName: String {
        reading.entityIdentifierFieldName
    }

    func transform(entity: U) throws -> T {
        try reading.transform(entity: entity)
    }

    func populate(entity _: U, from _: T, using _: NSManagedObjectContext) throws {
        throw SaveRefused()
    }
}
