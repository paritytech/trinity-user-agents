import Foundation
import EventKit
import Keystore_iOS

protocol GameCalendarServicing {
    func requestWriteAccess() async -> Bool
    func addEvent(for game: CalendarGameModel) throws
    func savedReminder() -> GameCalendarReminder?
    func saveReminder(_ reminder: GameCalendarReminder)
    func clearReminder()
}

final class GameCalendarService {
    let eventStore: EKEventStore
    let settings: SettingsManagerProtocol
    let logger: LoggerProtocol

    init(
        eventStore: EKEventStore,
        settings: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.eventStore = eventStore
        self.settings = settings
        self.logger = logger
    }
}

extension GameCalendarService: GameCalendarServicing {
    func requestWriteAccess() async -> Bool {
        do {
            return try await eventStore.requestWriteOnlyAccessToEvents()
        } catch {
            logger.error("Calendar write access request failed: \(error)")
            return false
        }
    }

    func addEvent(for game: CalendarGameModel) throws {
        let event = makeEvent(for: game)
        try eventStore.save(event, span: .thisEvent)
        logger.info("Event added to calendar: \(event)")
    }

    func savedReminder() -> GameCalendarReminder? {
        settings.gameCalendarReminder
    }

    func saveReminder(_ reminder: GameCalendarReminder) {
        settings.gameCalendarReminder = reminder
    }

    func clearReminder() {
        settings.gameCalendarReminder = nil
    }
}

private extension GameCalendarService {
    func makeEvent(for game: CalendarGameModel) -> EKEvent {
        let event = EKEvent(eventStore: eventStore)

        if let defaultCalendar = eventStore.defaultCalendarForNewEvents {
            event.calendar = defaultCalendar
        }

        event.title = game.title
        event.startDate = game.startDate
        event.endDate = game.endDate
        event.notes = game.notes

        if let remindBefore = game.remindBefore {
            event.addAlarm(EKAlarm(relativeOffset: -remindBefore))
        }

        return event
    }
}
