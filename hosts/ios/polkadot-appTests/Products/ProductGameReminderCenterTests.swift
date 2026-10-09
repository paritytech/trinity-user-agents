import Foundation
import FoundationExt
import Testing
import UIKit
import Keystore_iOS
import PolkadotUI
@testable import polkadot_app

@MainActor
@Suite("Product game reminder center")
struct ProductGameReminderCenterTests {
    private let productId = "game.dot"
    private let other = "other.dot"
    private let startsAt = Date(timeIntervalSince1970: 2_000_000_000)

    private func makeSUT(
        now: Date,
        alarmAvailable: Bool = true,
        settings: SettingsManagerProtocol = InMemorySettingsManager()
    ) -> (center: ProductGameReminderCenter, fakes: Fakes) {
        let fakes = Fakes(now: now, alarmAvailable: alarmAvailable)
        let center = ProductGameReminderCenter(
            dependencies: .init(
                services: fakes,
                settingsManager: settings,
                applicationState: fakes,
                applicationStateStreams: ApplicationStateStreamFactory(),
                productOpener: fakes,
                dateProvider: fakes
            )
        )
        center.start()
        return (center, fakes)
    }

    @Test("A schedule rings an alarm at the native lead time when AlarmKit is available")
    func scheduleRingsAlarm() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))

        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        #expect(center.slots == [.init(productId: productId, startsAt: startsAt)])
        #expect(fakes.alarm(for: productId).scheduled == [reminderCall(startsAt, productId)])
        #expect(fakes.notification(for: productId).scheduled.isEmpty)
    }

    @Test(
        "Without AlarmKit or without ringAlarm the reminder is a notification",
        arguments: [(true, false), (false, true)]
    )
    func scheduleFallsBackToNotification(ringAlarm: Bool, alarmAvailable: Bool) async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600), alarmAvailable: alarmAvailable)

        await center.schedule(productId: productId, startsAt: startsAt, ringAlarm: ringAlarm, addCalendarEvent: false)

        #expect(center.slots == [.init(productId: productId, startsAt: startsAt, ringAlarm: ringAlarm)])
        #expect(fakes.alarm(for: productId).scheduled.isEmpty)
        #expect(
            fakes.notification(for: productId).scheduled
                == [reminderCall(startsAt, productId)]
        )
    }

    @Test("Each product holds its own reminder, and only its own cancel drops it")
    func oneReminderPerProduct() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))
        let later = startsAt.addingTimeInterval(600)

        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        await center.schedule(productId: other, startsAt: later, addCalendarEvent: false)

        #expect(
            center.slots == [.init(productId: productId, startsAt: startsAt), .init(productId: other, startsAt: later)]
        )
        #expect(fakes.alarm(for: productId).scheduled == [reminderCall(startsAt, productId)])
        #expect(fakes.alarm(for: other).scheduled == [reminderCall(later, other)])

        center.cancel(productId: other)
        #expect(center.slots == [.init(productId: productId, startsAt: startsAt)])
        #expect(fakes.alarm(for: other).cancelCount == 1)
        #expect(fakes.alarm(for: productId).cancelCount == 0)

        center.cancel(productId: productId)
        #expect(center.slots.isEmpty)
        #expect(fakes.alarm(for: productId).cancelCount == 1)
    }

    @Test("A product replaces its own reminder with a new start")
    func productReplacesOwnReminder() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))
        let later = startsAt.addingTimeInterval(600)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        await center.schedule(productId: productId, startsAt: later, addCalendarEvent: false)

        #expect(center.slots == [.init(productId: productId, startsAt: later)])
        #expect(fakes.alarm(for: productId).scheduled.last == reminderCall(later, productId))
    }

    @Test("With the calendar flag and a start exactly an hour away, one calendar event is added")
    func addsCalendarEventOnce() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))

        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: true)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: true)

        let expected = RecordingCalendar.Event(
            title: String(localized: .Game.calendarEventGameTitle),
            startDate: startsAt,
            endDate: startsAt.addingTimeInterval(1_800),
            remindBefore: 300
        )
        #expect(fakes.calendar.added == [expected])
        #expect(center.slot(for: productId)?.calendarStartsAt == startsAt)
    }

    @Test(
        "No calendar event without the flag, for a start under an hour away, or without write access",
        arguments: [(false, 7_200.0, true), (true, 3_599.0, true), (true, 7_200.0, false)]
    )
    func noCalendarEvent(addCalendarEvent: Bool, secondsBeforeStart: TimeInterval, writeAccess: Bool) async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-secondsBeforeStart))
        fakes.calendar.writeAccess = writeAccess

        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: addCalendarEvent)

        #expect(fakes.calendar.added.isEmpty)
        #expect(center.slots == [.init(productId: productId, startsAt: startsAt)])
    }

    @Test("Rescheduling with ringAlarm drops the notification the same slot held")
    func alarmReplacesNotification() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-3_600))
        await center.schedule(productId: productId, startsAt: startsAt, ringAlarm: false, addCalendarEvent: false)
        let notificationCancels = fakes.notification(for: productId).cancelCount

        await center.schedule(productId: productId, startsAt: startsAt, ringAlarm: true, addCalendarEvent: false)

        #expect(center.slots == [.init(productId: productId, startsAt: startsAt, ringAlarm: true)])
        #expect(fakes.notification(for: productId).cancelCount == notificationCancels + 1)
        #expect(fakes.alarm(for: productId).scheduled == [reminderCall(startsAt, productId)])
    }

    @Test("The countdown pill is published only in the last five minutes, for the soonest start")
    func pillInLastFiveMinutes() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-6 * 60))
        let later = startsAt.addingTimeInterval(60)

        await center.schedule(productId: other, startsAt: later, addCalendarEvent: false)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        #expect(center.pill == nil)

        fakes.now = startsAt.addingTimeInterval(-4 * 60)
        center.refresh()
        #expect(center.pill == ProductGamePill(productId: productId, startsAt: startsAt))

        center.cancel(productId: productId)
        #expect(center.pill == ProductGamePill(productId: other, startsAt: later))

        center.cancel(productId: other)
        #expect(center.pill == nil)
    }

    @Test(
        "At the start the product opens in the foreground, long after it nothing opens, and the slot is dropped",
        arguments: [
            (UIApplication.State.active, 1.0, true),
            (.inactive, 1.0, true),
            (.active, 600.0, false),
            (.background, 600.0, false)
        ]
    )
    func opensAtStart(state: UIApplication.State, secondsAfterStart: TimeInterval, opens: Bool) async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60))
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        fakes.applicationState = state
        fakes.now = startsAt.addingTimeInterval(secondsAfterStart)
        center.refresh()

        #expect(fakes.opened == (opens ? [productId] : []))
        #expect(center.slots.isEmpty)
        #expect(center.pill == nil)
    }

    @Test("A start opens its product and leaves another product's reminder in place")
    func opensOneProductAmongMany() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60))
        let later = startsAt.addingTimeInterval(3_600)
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)
        await center.schedule(productId: other, startsAt: later, addCalendarEvent: false)

        fakes.now = startsAt.addingTimeInterval(1)
        center.refresh()

        #expect(fakes.opened == [productId])
        #expect(center.slots == [.init(productId: other, startsAt: later)])
        #expect(fakes.alarm(for: other).cancelCount == 0)
    }

    @Test("Reached in the background, the start keeps the slot and opens once the app is active within the grace")
    func opensWhenActiveAfterBackgroundStart() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-60))
        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: false)

        fakes.applicationState = .background
        fakes.now = startsAt.addingTimeInterval(1)
        center.refresh()
        #expect(fakes.opened.isEmpty)
        #expect(center.slots == [.init(productId: productId, startsAt: startsAt)])
        #expect(center.pill == nil)

        fakes.applicationState = .active
        fakes.now = startsAt.addingTimeInterval(5)
        center.refresh()
        #expect(fakes.opened == [productId])
        #expect(center.slots.isEmpty)
    }

    @Test("The slots, including ringAlarm, survive a relaunch and the pill comes back")
    func restoresAfterRelaunch() async {
        let settings = InMemorySettingsManager()
        let (first, _) = makeSUT(now: startsAt.addingTimeInterval(-3_600), settings: settings)
        await first.schedule(productId: productId, startsAt: startsAt, ringAlarm: false, addCalendarEvent: false)

        let (relaunched, _) = makeSUT(now: startsAt.addingTimeInterval(-60), settings: settings)

        #expect(relaunched.slots == [.init(productId: productId, startsAt: startsAt, ringAlarm: false)])
        #expect(relaunched.pill == ProductGamePill(productId: productId, startsAt: startsAt))
    }

    @Test("A cancel while calendar access is being asked adds no event")
    func cancelDuringCalendarAccessAddsNoEvent() async {
        let (center, fakes) = makeSUT(now: startsAt.addingTimeInterval(-7_200))
        fakes.calendar.onRequestWriteAccess = { [productId] in center.cancel(productId: productId) }

        await center.schedule(productId: productId, startsAt: startsAt, addCalendarEvent: true)

        #expect(center.slots.isEmpty)
        #expect(fakes.calendar.added.isEmpty)
    }
}

private extension ProductGameReminderCenter {
    func schedule(productId: String, startsAt: Date, addCalendarEvent: Bool) async {
        await schedule(productId: productId, startsAt: startsAt, ringAlarm: true, addCalendarEvent: addCalendarEvent)
    }
}

/// The reminder call the center makes for a product's start, at the native 20 s lead time.
private func reminderCall(_ startsAt: Date, _ productId: String) -> RecordingGameReminder.Call {
    .init(gameDate: startsAt, target: .product(productId), timingSeconds: 20)
}

// MARK: - Fakes

private final class Fakes: @unchecked Sendable {
    var now: Date
    var applicationState: UIApplication.State = .active
    var opened: [String] = []
    let calendar = RecordingCalendar()
    private let alarmAvailable: Bool
    private var alarms: [String: RecordingGameReminder] = [:]
    private var notifications: [String: RecordingGameReminder] = [:]

    init(now: Date, alarmAvailable: Bool) {
        self.now = now
        self.alarmAvailable = alarmAvailable
    }

    func alarm(for productId: String) -> RecordingGameReminder {
        reminder(in: &alarms, for: productId)
    }

    func notification(for productId: String) -> RecordingGameReminder {
        reminder(in: &notifications, for: productId)
    }

    private func reminder(
        in store: inout [String: RecordingGameReminder],
        for productId: String
    ) -> RecordingGameReminder {
        if let reminder = store[productId] {
            return reminder
        }
        let reminder = RecordingGameReminder()
        store[productId] = reminder
        return reminder
    }
}

extension Fakes: ProductGameReminderServicesMaking, ApplicationStateProviding, ProductOpening, CurrentDateProviding {
    func makeAlarm(for productId: String) -> (any GameStartReminderServicing)? {
        alarmAvailable ? alarm(for: productId) : nil
    }

    func makeNotification(for productId: String) -> any GameStartReminderServicing {
        notification(for: productId)
    }

    func makeCalendar() -> any GameCalendarServicing {
        calendar
    }

    func open(productId: String) {
        opened.append(productId)
    }
}

private final class RecordingGameReminder: GameStartReminderServicing {
    struct Call: Equatable {
        let gameDate: Date
        let target: GameReminderTarget
        let timingSeconds: Int
    }

    private(set) var scheduled: [Call] = []
    private(set) var cancelCount = 0

    func scheduleReminder(gameDate: Date, target: GameReminderTarget, timingSeconds: Int) {
        scheduled.append(Call(gameDate: gameDate, target: target, timingSeconds: timingSeconds))
    }

    func cancelReminder() {
        cancelCount += 1
    }
}

private final class RecordingCalendar: GameCalendarServicing {
    struct Event: Equatable {
        let title: String
        let startDate: Date
        let endDate: Date
        let remindBefore: TimeInterval?
    }

    var writeAccess = true
    var onRequestWriteAccess: (@MainActor () -> Void)?
    private(set) var added: [Event] = []

    func requestWriteAccess() async -> Bool {
        await onRequestWriteAccess?()
        return writeAccess
    }

    func addEvent(for game: CalendarGameModel) throws {
        added.append(
            Event(title: game.title, startDate: game.startDate, endDate: game.endDate, remindBefore: game.remindBefore)
        )
    }

    func savedReminder() -> GameCalendarReminder? {
        nil
    }

    func saveReminder(_: GameCalendarReminder) {}

    func clearReminder() {}
}
