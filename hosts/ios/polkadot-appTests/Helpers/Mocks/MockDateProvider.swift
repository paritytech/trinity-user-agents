import Foundation
import FoundationExt

/// A `DateProviding` whose date moves only when a test moves it.
actor MockDateProvider: DateProviding {
    private var current: Date

    init(now: Date = Date(timeIntervalSince1970: 1_700_000_000)) {
        current = now
    }

    func read() async -> Date {
        current
    }

    func advance(by interval: TimeInterval) {
        current = current.addingTimeInterval(interval)
    }
}
