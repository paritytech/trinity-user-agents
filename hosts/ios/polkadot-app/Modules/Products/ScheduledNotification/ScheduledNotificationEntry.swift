import Foundation
import Operation_iOS

struct ScheduledNotificationEntry: Equatable {
    let productId: String
    let notificationId: UInt32

    var unIdentifier: String {
        "product:\(productId):\(notificationId)"
    }
}

extension ScheduledNotificationEntry: Operation_iOS.Identifiable {
    var identifier: String {
        "\(productId):\(notificationId)"
    }
}
