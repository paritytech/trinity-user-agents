import Foundation

/// Supplies the current time behind a protocol so time-dependent logic (cache TTLs, scheduling)
/// can be driven deterministically in tests.
public protocol DateProviding: Sendable {
    func read() async -> Date
}

public struct NowDateProvider: DateProviding {
    public init() {}

    public func read() async -> Date {
        Date()
    }
}
