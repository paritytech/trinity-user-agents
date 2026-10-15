import Foundation

/// Supplies the current time synchronously, for logic that has to decide in one step without
/// suspending, such as UI state recomputed on the main actor.
public protocol CurrentDateProviding: Sendable {
    var now: Date { get }
}

extension NowDateProvider: CurrentDateProviding {
    public var now: Date {
        Date()
    }
}
