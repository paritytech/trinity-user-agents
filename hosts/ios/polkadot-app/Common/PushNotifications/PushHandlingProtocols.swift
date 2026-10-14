import Foundation
import UserNotifications

protocol PushNotificationTapHandling: AnyObject {
    func handle(response: UNNotificationResponse, completion: @escaping () -> Void)
}
