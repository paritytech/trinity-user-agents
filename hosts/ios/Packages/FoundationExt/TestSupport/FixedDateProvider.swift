import Foundation
import FoundationExt

public struct FixedDateProvider: DateProviding {
    public let date: Date

    public init(_ date: Date) {
        self.date = date
    }

    public func read() async -> Date {
        date
    }
}
