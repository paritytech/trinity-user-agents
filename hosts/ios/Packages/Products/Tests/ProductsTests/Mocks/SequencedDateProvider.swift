import Foundation
import FoundationExt
import os

final class SequencedDateProvider: DateProviding, @unchecked Sendable {
    private let dates: OSAllocatedUnfairLock<[Date]>

    init(_ dates: [Date]) {
        self.dates = OSAllocatedUnfairLock(initialState: dates)
    }

    func read() async -> Date {
        dates.withLock { dates in
            dates.count > 1 ? dates.removeFirst() : dates[0]
        }
    }
}
