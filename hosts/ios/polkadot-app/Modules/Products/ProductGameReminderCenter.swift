import AsyncExtensions
import Foundation
import FoundationExt
import Keystore_iOS
import Products
import UIKit

/// One game reminder per product, driving its alarm, the calendar event, the countdown pill it
/// publishes and opening the product at the start. A schedule replaces the reminder the same
/// product holds.
@MainActor
final class ProductGameReminderCenter: ProductGameReminderScheduling, ProductGamePillProviding {
    struct Slot: Codable, Equatable {
        let productId: ProductId
        let startsAt: Date
        /// False delivers a notification even when AlarmKit is authorized.
        var ringAlarm = true
        /// The start already in the calendar, so a repeat for it adds nothing.
        var calendarStartsAt: Date?
    }

    struct Dependencies {
        let services: ProductGameReminderServicesMaking
        let settingsManager: SettingsManagerProtocol
        let applicationState: ApplicationStateProviding
        let applicationStateStreams: ApplicationStateStreamFactory
        let productOpener: ProductOpening
        let dateProvider: CurrentDateProviding
    }

    static func makeDefault() -> ProductGameReminderCenter {
        ProductGameReminderCenter(
            dependencies: Dependencies(
                services: ProductGameReminderServicesFactory(
                    settingsManager: SettingsManager.shared,
                    localNotificationService: UserNotificationService.shared
                ),
                settingsManager: SettingsManager.shared,
                applicationState: UIApplication.shared,
                applicationStateStreams: ApplicationStateStreamFactory(),
                productOpener: ProductOpener(),
                dateProvider: NowDateProvider()
            )
        )
    }

    /// How late after the start a wake-up may run and still open the product.
    private static let openGrace: TimeInterval = 30

    /// Calendar events are added only for games at least this far away, as for the native game.
    private static let calendarLeadTime: TimeInterval = .secondsInHour
    private static let calendarEventDuration: TimeInterval = 30 * .secondsInMinute
    private static let calendarRemindBefore: TimeInterval = 5 * .secondsInMinute

    fileprivate struct Reminders {
        let alarm: (any GameStartReminderServicing)?
        let notification: any GameStartReminderServicing
    }

    private let services: ProductGameReminderServicesMaking
    private let settingsManager: SettingsManagerProtocol
    private let applicationState: ApplicationStateProviding
    private let applicationStateStreams: ApplicationStateStreamFactory
    private let productOpener: ProductOpening
    private let dateProvider: CurrentDateProviding

    /// One pair per product: AlarmKit serialises its calls per instance.
    private var reminders: [ProductId: Reminders] = [:]
    private let pillSubject = AsyncCurrentValueSubject<ProductGamePill?>(nil)
    private var wakeUp: Task<Void, Never>?
    private var becameActive: Task<Void, Never>?

    init(dependencies: Dependencies) {
        services = dependencies.services
        settingsManager = dependencies.settingsManager
        applicationState = dependencies.applicationState
        applicationStateStreams = dependencies.applicationStateStreams
        productOpener = dependencies.productOpener
        dateProvider = dependencies.dateProvider
    }

    /// Held reminders, soonest first.
    var slots: [Slot] {
        guard let data = settingsManager.anyValue(for: SettingsKey.productGameReminders.rawValue) as? Data,
              let slots = try? JSONDecoder().decode([Slot].self, from: data) else {
            return []
        }
        return slots.sorted { $0.startsAt < $1.startsAt }
    }

    func slot(for productId: ProductId) -> Slot? {
        slots.first { $0.productId == productId }
    }

    var pill: ProductGamePill? {
        pillSubject.value
    }

    func start() {
        observeBecomingActive()
        refresh()
    }

    func pillStream() -> AnyAsyncSequence<ProductGamePill?> {
        pillSubject.eraseToAnyAsyncSequence()
    }

    func schedule(
        productId: ProductId,
        startsAt: Date,
        ringAlarm: Bool,
        addCalendarEvent: Bool
    ) async {
        let next = Slot(
            productId: productId,
            startsAt: Date(timeIntervalSince1970: startsAt.timeIntervalSince1970.rounded(.down)),
            ringAlarm: ringAlarm,
            calendarStartsAt: slot(for: productId)?.calendarStartsAt
        )
        deliver(next)
        refresh()

        if addCalendarEvent {
            await addCalendarEventIfNeeded(next)
        }
    }

    func cancel(productId: ProductId) {
        guard slot(for: productId) != nil else {
            return
        }
        drop(productId)
        refresh()
    }

    func refresh() {
        wakeUp?.cancel()
        wakeUp = nil

        let now = dateProvider.now
        var pill: Slot?
        var wakeAt: Date?
        var opened = false

        for slot in slots {
            guard now < slot.startsAt else {
                let isLate = now.timeIntervalSince(slot.startsAt) >= Self.openGrace
                if applicationState.applicationState == .background, !isLate {
                    // Becoming active within the grace opens the product.
                    wakeAt = earliest(wakeAt, slot.startsAt.addingTimeInterval(Self.openGrace))
                    continue
                }
                drop(slot.productId)
                if !isLate, !opened {
                    opened = true
                    productOpener.open(productId: slot.productId)
                }
                continue
            }

            let pillAt = slot.startsAt.addingTimeInterval(-GameRoomPillState.Constants.startingPillLeadTime)
            if pill == nil, now >= pillAt {
                pill = slot
            }
            wakeAt = earliest(wakeAt, now < pillAt ? pillAt : slot.startsAt)
        }

        publish(pill)
        if let wakeAt {
            wake(at: wakeAt, from: now)
        }
    }
}

private extension ProductGameReminderCenter {
    func reminders(for productId: ProductId) -> Reminders {
        if let reminders = reminders[productId] {
            return reminders
        }
        let reminders = Reminders(
            alarm: services.makeAlarm(for: productId),
            notification: services.makeNotification(for: productId)
        )
        self.reminders[productId] = reminders
        return reminders
    }

    func deliver(_ next: Slot) {
        let reminders = reminders(for: next.productId)
        let (delivery, unused): (any GameStartReminderServicing, (any GameStartReminderServicing)?) =
            if next.ringAlarm, let alarm = reminders.alarm {
                (alarm, reminders.notification)
            } else {
                (reminders.notification, reminders.alarm)
            }
        unused?.cancelReminder()
        store(next)

        delivery.scheduleReminder(
            gameDate: next.startsAt,
            target: .product(next.productId),
            timingSeconds: settingsManager.gameAlarmTimingSeconds
        )
    }

    /// Write-only: events are never removed, so a repeat for the same start adds nothing.
    func addCalendarEventIfNeeded(_ slot: Slot) async {
        guard slot.startsAt.timeIntervalSince(dateProvider.now) >= Self.calendarLeadTime,
              slot.calendarStartsAt != slot.startsAt else {
            return
        }
        let calendar = services.makeCalendar()
        guard await calendar.requestWriteAccess() else {
            return
        }
        // The access prompt may outlive the reminder it was asked for, or an overlapping schedule
        // may have added the event meanwhile.
        guard var held = self.slot(for: slot.productId),
              held.startsAt == slot.startsAt,
              held.calendarStartsAt != slot.startsAt else {
            return
        }

        let event = CalendarGameModel(
            title: String(localized: .Game.calendarEventGameTitle),
            startDate: slot.startsAt,
            endDate: slot.startsAt.addingTimeInterval(Self.calendarEventDuration),
            notes: nil,
            remindBefore: Self.calendarRemindBefore
        )
        do {
            try calendar.addEvent(for: event)
        } catch {
            Logger.shared.error("Failed to add product game to calendar: \(error)")
            return
        }
        held.calendarStartsAt = slot.startsAt
        store(held)
    }

    func drop(_ productId: ProductId) {
        store(slots.filter { $0.productId != productId })
        let reminders = reminders(for: productId)
        reminders.alarm?.cancelReminder()
        reminders.notification.cancelReminder()
    }

    func store(_ slot: Slot) {
        store(slots.filter { $0.productId != slot.productId } + [slot])
    }

    func store(_ slots: [Slot]) {
        guard !slots.isEmpty, let data = try? JSONEncoder().encode(slots) else {
            settingsManager.removeValue(for: .productGameReminders)
            return
        }
        settingsManager.set(anyValue: data, for: SettingsKey.productGameReminders.rawValue)
    }

    func earliest(_ current: Date?, _ candidate: Date) -> Date {
        current.map { min($0, candidate) } ?? candidate
    }

    func publish(_ slot: Slot?) {
        let pill = slot.map { ProductGamePill(productId: $0.productId, startsAt: $0.startsAt) }
        guard pill != pillSubject.value else {
            return
        }
        pillSubject.send(pill)
    }

    /// A wake-up timer can fire while the app is still resuming, so becoming active re-checks the start.
    func observeBecomingActive() {
        guard becameActive == nil else {
            return
        }
        let events = applicationStateStreams.stream(for: .didBecomeActive)
        becameActive = Task { [weak self] in
            for await _ in events {
                guard let self else {
                    return
                }
                refresh()
            }
        }
    }

    func wake(at date: Date, from now: Date) {
        let delay = date.timeIntervalSince(now)
        wakeUp = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled else {
                return
            }
            self?.refresh()
        }
    }
}
