import Foundation
import NotificationCenter

enum NotificationAccessStatus: Equatable {
    case allowed
    case notAllowed(denied: Bool)

    init(status: UNAuthorizationStatus) {
        switch status {
        case .authorized,
             .provisional,
             .ephemeral:
            self = .allowed
        default:
            self = .notAllowed(denied: status == .denied)
        }
    }
}

extension NotificationAccessStatus {
    var accessGranted: Bool {
        if case .allowed = self {
            true
        } else {
            false
        }
    }

    var denied: Bool {
        if case .notAllowed(denied: true) = self {
            true
        } else {
            false
        }
    }
}
