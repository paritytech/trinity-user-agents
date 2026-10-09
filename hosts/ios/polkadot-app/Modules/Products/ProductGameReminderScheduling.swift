import AsyncExtensions
import Foundation
import Products

/// Schedules and cancels a product's next-game reminder.
@MainActor
protocol ProductGameReminderScheduling: AnyObject {
    /// Replaces the reminder the product already holds. `ringAlarm` false delivers an ordinary
    /// notification, not an alarm.
    func schedule(
        productId: ProductId,
        startsAt: Date,
        ringAlarm: Bool,
        addCalendarEvent: Bool
    ) async

    func cancel(productId: ProductId)
}

/// The countdown a product's reminder asks the UI to show in the last minutes before its game.
struct ProductGamePill: Equatable {
    let productId: ProductId
    let startsAt: Date
}

/// Publishes the pill for the soonest upcoming game, or nil. Where and whether it is shown is the
/// UI's decision: it knows which product is on screen.
@MainActor
protocol ProductGamePillProviding: AnyObject {
    func start()
    func pillStream() -> AnyAsyncSequence<ProductGamePill?>
}
