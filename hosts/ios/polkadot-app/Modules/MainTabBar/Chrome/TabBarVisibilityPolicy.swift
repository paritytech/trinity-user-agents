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
        switch override {
        case .tucked:
            .tucked
        case .shown:
            .shown
        case .none:
            state(for: derived)
        }
    }

    static func contributesClearance(isTabRoot: Bool) -> Bool {
        isTabRoot
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
