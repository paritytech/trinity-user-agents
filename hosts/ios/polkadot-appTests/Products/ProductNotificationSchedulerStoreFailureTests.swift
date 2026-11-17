import struct TrUAPIHost.ProductId
import Foundation
import FoundationExt
import IssueMonitoring
import Products
import Testing
@testable import polkadot_app

@Suite("Product notification scheduler store failures")
struct ProductNotificationSchedulerStoreFailureTests {
    private static let product = "example.dot"

    private let productId = try! ProductId(Self.product)
    private let request = ScheduledNotificationRequest(text: "Your game starts soon", deeplink: nil, scheduledAtMs: nil)
    private let center = FakeScheduledNotificationCenter()
    private let recorder = RecordingHostEventRecorder()
    private let storageFacade = StoreRefusingStorageFacade()

    private let scheduler: ProductNotificationScheduler

    init() {
        scheduler = ProductNotificationScheduler(
            storageFacade: storageFacade,
            notificationCenter: center,
            diagnostics: ProductNotificationDiagnostics.recording(into: recorder),
            applicationStateStreams: ApplicationStateStreamFactory(notificationCenter: NotificationCenter())
        )
    }

    private func event(_ step: String, _ id: UInt32) -> HostEvent {
        expectedProductNotificationHostEvent(step, productId: Self.product, id: id)
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

    private func pendingIdentifiers() async -> [String] {
        await center.pendingNotificationRequests().map(\.identifier)
    }

    private func book() async throws -> UInt32 {
        try await scheduler.schedule(productId: productId, request: request)
    }

    @Test func aRefusedStoreReadRecordsNotScheduledWithoutAnIdAndStillThrows() async throws {
        let id = try await book()
        storageFacade.refuseReads()

        await #expect(throws: StoreRefusingStorageFacade.ReadRefused.self) {
            _ = try await book()
        }

        #expect(recorder.events == [event("scheduled", id), notScheduled(nil, reason: "sweep_failed")])
    }

    @Test func aRefusedStoreWriteLeavesNothingPending() async throws {
        storageFacade.refuseWrites()

        await #expect(throws: StoreRefusingStorageFacade.WriteRefused.self) {
            _ = try await book()
        }

        let id = try #require(center.lastAttemptedBookingId)
        #expect(await pendingIdentifiers() == [])
        #expect(recorder.events == [notScheduled(id, reason: "store_failed")])
    }

    @Test func aFailedPruneReportsTheStaleBookingOnlyOnTheRetry() async throws {
        let lostId = try await book()
        center.losePending()
        storageFacade.refuseWrites()

        await #expect(throws: StoreRefusingStorageFacade.WriteRefused.self) {
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

    @Test func aCancelTakesTheBookingOffTheScreenWhenTheStoreFails() async throws {
        let id = try await book()
        storageFacade.refuseReads()

        await #expect(throws: StoreRefusingStorageFacade.ReadRefused.self) {
            try await scheduler.cancel(productId: productId, notificationId: id)
        }

        #expect(await pendingIdentifiers() == [])
    }

    @Test func aStoreThatFailsAfterTheSweepRecordsReadFailed() async throws {
        let id = try await book()
        storageFacade.refuseReads(after: 1)

        await #expect(throws: StoreRefusingStorageFacade.ReadRefused.self) {
            _ = try await book()
        }

        #expect(recorder.events == [event("scheduled", id), notScheduled(nil, reason: "read_failed")])
    }

    @Test func aCancelWhoseDeleteFailsRecordsNoCancel() async throws {
        let id = try await book()
        storageFacade.refuseWrites()

        await #expect(throws: StoreRefusingStorageFacade.WriteRefused.self) {
            try await scheduler.cancel(productId: productId, notificationId: id)
        }

        #expect(recorder.events == [event("scheduled", id)])
    }

    @Test func aCancelAllWhoseDeleteFailsRecordsNoCancel() async throws {
        let id = try await book()
        storageFacade.refuseWrites()

        await #expect(throws: StoreRefusingStorageFacade.WriteRefused.self) {
            try await scheduler.cancelAll(forProductId: productId)
        }

        #expect(await pendingIdentifiers() == [])
        #expect(recorder.events == [event("scheduled", id)])
    }
}
