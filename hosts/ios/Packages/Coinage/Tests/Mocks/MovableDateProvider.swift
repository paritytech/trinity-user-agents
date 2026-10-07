import Foundation
import FoundationExt
import os

/// A `DateProviding` whose date moves only when a test moves it.
final class MovableDateProvider: DateProviding, Sendable {
    private let current: OSAllocatedUnfairLock<Date>

    init(now: Date) {
        current = OSAllocatedUnfairLock(initialState: now)
    }

    func advance(by interval: TimeInterval) {
        current.withLock { $0 = $0.addingTimeInterval(interval) }
    }

    func read() async -> Date {
        current.withLock { $0 }
    }
}
