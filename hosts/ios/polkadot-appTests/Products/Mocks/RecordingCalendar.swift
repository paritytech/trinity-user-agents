import Foundation
import Products
@testable import polkadot_app

final class RecordingCalendar: GameCalendarServicing {
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
