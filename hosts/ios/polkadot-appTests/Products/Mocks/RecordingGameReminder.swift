import Foundation
import Products
@testable import polkadot_app

final class RecordingGameReminder: GameStartReminderServicing {
    struct Call: Equatable {
        let gameDate: Date
        let productId: ProductId
    }

    private(set) var scheduled: [Call] = []
    private(set) var cancelCount = 0

    func scheduleReminder(gameDate: Date, productId: ProductId) {
        scheduled.append(Call(gameDate: gameDate, productId: productId))
    }

    func cancelReminder() {
        cancelCount += 1
    }
}

/// The reminder call the center makes for a product's start, which rings at the start itself.
func reminderCall(_ startsAt: Date, _ productId: String) -> RecordingGameReminder.Call {
    .init(gameDate: startsAt, productId: productId)
}
