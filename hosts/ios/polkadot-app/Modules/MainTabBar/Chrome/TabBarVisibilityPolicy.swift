import Foundation

enum TabBarVisibilityState: Equatable {
    case shown
    case folded
    case hidden
}

enum TabBarFoldOverride: Equatable {
    case none
    case folded
    case shown
}

enum TabBarFoldDerived: Equatable {
    case none
    case folded
    case hidden
}

enum TabBarVisibilityPolicy {
    static func state(
        isTabRoot: Bool,
        derived: TabBarFoldDerived,
        override: TabBarFoldOverride
    ) -> TabBarVisibilityState {
        guard !isTabRoot else {
            return .shown
        }

        switch override {
        case .folded:
            return .folded
        case .shown:
            return .shown
        case .none:
            return state(for: derived)
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
        case .folded:
            .folded
        case .hidden:
            .hidden
        }
    }
}
