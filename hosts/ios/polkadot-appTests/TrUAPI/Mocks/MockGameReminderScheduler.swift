import Foundation
import Products
@testable import polkadot_app

@MainActor
final class MockGameReminderScheduler: ProductGameReminderScheduling {
    struct Call: Equatable {
        let productId: ProductId
        let startsAt: Date
        let ringAlarm: Bool
        let addCalendarEvent: Bool
    }

    private(set) var scheduled: [Call] = []

    func schedule(
        productId: ProductId,
        startsAt: Date,
        ringAlarm: Bool,
        addCalendarEvent: Bool
    ) async {
        scheduled.append(
            Call(productId: productId, startsAt: startsAt, ringAlarm: ringAlarm, addCalendarEvent: addCalendarEvent)
        )
    }

    func cancel(productId _: ProductId) {}
}
