import Foundation

/// The user-chosen privacy preset. The raw value is the persisted key.
public enum RecyclingStrategyType: String, CaseIterable, Equatable, Sendable {
    case minPrivacy
    case balanced
    case maxPrivacy
}

public extension RecyclingStrategyType {
    /// Whether the user may spend value that is still gaining privacy, once they confirm it.
    var allowsConfirmedSpend: Bool {
        self != .maxPrivacy
    }
}
