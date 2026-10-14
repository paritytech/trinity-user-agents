import Foundation
import FoundationExt

/// A clock frozen at a fixed instant, so a retry window's start is exact in assertions.
struct StubDateProvider: DateProviding {
    let date: Date

    init(_ date: Date) {
        self.date = date
    }

    func read() async -> Date {
        date
    }
}
