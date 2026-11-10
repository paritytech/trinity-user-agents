import Foundation

enum TabBarVisibilityState: Equatable {
    case shown
    case tucked
    case hidden
}

enum TabBarFoldOverride: Equatable {
    case none
    case tucked
    case shown
}

enum TabBarFoldDerived: Equatable {
    case none
    case tucked
    case hidden
}

enum TabBarVisibilityPolicy {
    static func state(derived: TabBarFoldDerived, override: TabBarFoldOverride) -> TabBarVisibilityState {
        guard derived != .hidden else {
            return .hidden
        }
        switch override {
        case .tucked:
            return .tucked
        case .shown:
            return .shown
        case .none:
            return derived.defaultState
        }
    }
}

extension TabBarVisibilityState {
    /// The override a new screen starts with when the bar comes from this state. Only a tucked bar carries over,
    /// so the new screen's derived state decides otherwise.
    var carriedOverride: TabBarFoldOverride {
        self == .tucked ? .tucked : .none
    }

    /// A swap between tucked and hidden keeps the bar off screen at both ends, so it is written without animation.
    func isOffScreenSwap(to other: TabBarVisibilityState) -> Bool {
        (self == .tucked && other == .hidden) || (self == .hidden && other == .tucked)
    }
}

extension TabBarFoldDerived {
    /// The state a derived fold shows when the user has not overridden it.
    var defaultState: TabBarVisibilityState {
        switch self {
        case .none:
            .shown
        case .tucked:
            .tucked
        case .hidden:
            .hidden
        }
    }
}
