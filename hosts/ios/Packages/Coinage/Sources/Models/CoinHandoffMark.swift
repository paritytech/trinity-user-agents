import Foundation

/// Whether — and how far along — a coin has been handed off to a peer. Insert-only and monotonic:
/// once handed off a coin never returns to `.none`, so any non-`.none` value means handed off.
public enum CoinHandoffMark: Int16, Sendable, Equatable {
    case none = 0
    /// Written before the keys leave the device, ahead of statement-store submission.
    case pending = 1
    /// The carrying message has been committed to the statement store.
    case committed = 2
}
