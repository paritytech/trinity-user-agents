import Foundation

enum RootDestination: Equatable {
    case selectTheme
    case onboarding
    case restoreFromCloud
    case usernameCheck
    case dashboard
    case broken
}

extension RootDestination {
    var impliesEstablishedUser: Bool {
        switch self {
        case .dashboard:
            true
        default:
            false
        }
    }
}
