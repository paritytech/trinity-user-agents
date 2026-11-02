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
            return state(for: derived)
        }
    }

    static func contributesClearance(isTabRoot: Bool) -> Bool {
        isTabRoot
    }

    /// The override a new screen starts with when the bar comes from `previous`. Only a tucked bar carries over,
    /// so the new screen's derived state decides otherwise.
    static func carriedOverride(from previous: TabBarVisibilityState) -> TabBarFoldOverride {
        previous == .tucked ? .tucked : .none
    }

    /// A swap between tucked and hidden keeps the bar off screen at both ends, so it is written without animation.
    static func isOffScreenSwap(from previous: TabBarVisibilityState, to resolved: TabBarVisibilityState) -> Bool {
        (previous == .tucked && resolved == .hidden) || (previous == .hidden && resolved == .tucked)
    }
}

private extension TabBarVisibilityPolicy {
    static func state(for derived: TabBarFoldDerived) -> TabBarVisibilityState {
        switch derived {
        case .none:
            .shown
        case .tucked:
            .tucked
        case .hidden:
            .hidden
        }
    }
}
