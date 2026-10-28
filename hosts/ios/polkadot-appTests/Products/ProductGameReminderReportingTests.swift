import Foundation
import Keystore_iOS
import Testing
@testable import polkadot_app

@MainActor
@Suite("Product game reminder center reporting")
struct ProductGameReminderReportingTests {
    private let productId = "game.dot"
    private let other = "other.dot"
    private let startsAt = Date(timeIntervalSince1970: 2_000_000_000)

    private func makeSUT(
        now: Date,
        alarmAvailable: Bool = true,
        settings: SettingsManagerProtocol = InMemorySettingsManager()
    ) -> (center: ProductGameReminderCenter, fakes: GameReminderCenterFakes) {
        GameReminderCenterFakes.makeCenter(now: now, alarmAvailable: alarmAvailable, settings: settings)
    }

    @Test("A product's cancel is reported with its start; a start it never held reports nothing")
    func reportsCancel() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        center.cancel(productId: other)
        center.cancel(productId: productId)

        #expect(
            fakes.recorder.events == [
                expectedReminderHostEvent(
                    "cancelled",
                    productId: productId,
                    startsAt: startsAt,
                    occurredAt: startsAt.addingTimeInterval(-3_600),
                    tags: ["host.app_state": "active"]
                )
            ]
        )
    }

    @Test("Opening at the start is reported as an auto-open with the app state")
    func reportsAutoOpen() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60))
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        fakes.applicationState = .inactive
        fakes.now = startsAt.addingTimeInterval(2)
        center.refresh()

        #expect(
            fakes.recorder.events == [
                expectedReminderHostEvent(
                    "product_opened",
                    productId: productId,
                    startsAt: startsAt,
                    occurredAt: startsAt.addingTimeInterval(2),
                    tags: ["reminder.open_source": "auto_open", "host.app_state": "inactive"]
                )
            ]
        )
    }

    @Test("An auto-open that finds the product already on screen is not reported")
    func autoOpenWithoutNavigationIsNotReported() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60))
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        fakes.openNavigates = false

        fakes.now = startsAt.addingTimeInterval(2)
        center.refresh()

        #expect(fakes.opened == [productId])
        #expect(fakes.recorder.events.isEmpty)
    }

    @Test("A ringing alarm is reported once, for the product that holds it")
    func reportsFiredAlarmOnce() async throws {
        let settings = InMemorySettingsManager()
        let alarmId = UUID()
        let otherAlarmId = UUID()
        settings.set(string: alarmId.uuidString, for: GameReminderStorageKeys.product(productId).alarmId)
        settings.set(string: otherAlarmId.uuidString, for: GameReminderStorageKeys.product(other).alarmId)
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60), settings: settings)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        await center.schedule(productId: other, startsAt: startsAt.addingTimeInterval(60), addCalendarEvent: false)
        fakes.now = startsAt.addingTimeInterval(1)

        // The stream is read in order, so the last product's ring arriving proves the repeat was read first.
        for alarmIds: Set<UUID> in [[UUID()], [alarmId], [alarmId], [otherAlarmId]] {
            fakes.alarmAlerts.yield(alarmIds)
        }
        try await waitUntil { fakes.recorder.events.count == 2 }

        #expect(
            fakes.recorder.events == [
                expectedReminderHostEvent(
                    "fired",
                    productId: productId,
                    startsAt: startsAt,
                    occurredAt: startsAt.addingTimeInterval(1),
                    tags: ["reminder.delivery": "alarm", "host.app_state": "active"]
                ),
                expectedReminderHostEvent(
                    "fired",
                    productId: other,
                    startsAt: startsAt.addingTimeInterval(60),
                    occurredAt: startsAt.addingTimeInterval(1),
                    tags: ["reminder.delivery": "alarm", "host.app_state": "active"]
                )
            ]
        )
    }

    @Test("A ringing alarm is recorded before refresh drops its past slot")
    func recordsFiredBeforeDroppingPastSlot() async {
        let settings = InMemorySettingsManager()
        let alarmId = UUID()
        settings.set(string: alarmId.uuidString, for: GameReminderStorageKeys.product(productId).alarmId)
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60), settings: settings)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        fakes.currentAlertingAlarms = [alarmId]
        fakes.now = startsAt.addingTimeInterval(1)

        center.refresh()

        #expect(fakes.recorder.events.map(\.name) == ["host.reminder.fired", "host.reminder.product_opened"])
        #expect(center.slots.isEmpty)
    }

    @Test("With no alarm ringing, a refresh does not look up the held reminders' alarms")
    func refreshWithoutRingingAlarmReadsNoAlarmId() async {
        let settings = ReadCountingSettingsManager()
        settings.set(string: UUID().uuidString, for: GameReminderStorageKeys.product(productId).alarmId)
        let (center, _) = makeSUT(now: startsAt.addingTimeInterval(-60), settings: settings)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        center.refresh()

        #expect(settings.stringReads.isEmpty)
    }
}
