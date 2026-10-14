import Foundation
import Products

extension OSPermissionStatus {
    init(notificationStatus: NotificationAccessStatus) {
        switch notificationStatus {
        case let .notAllowed(denied):
            self = denied ? .denied : .notDetermined
        case .allowed:
            self = .allowed
        @unknown default:
            self = .notDetermined
        }
    }
}
