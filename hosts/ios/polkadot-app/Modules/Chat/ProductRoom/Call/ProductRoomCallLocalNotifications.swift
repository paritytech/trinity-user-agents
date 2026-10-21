import Foundation
import UserNotifications

/// The local notification of a ringing room, as a time-sensitive notification of the products source. A tap opens
/// the link of the room through the push route of the app. The app has no time-sensitive entitlement yet, so iOS
/// delivers it as an active notification.
struct ProductRoomCallLocalNotifications: ProductRoomCallNotifying {
    func notify(_ notification: ProductRoomCallNotification) async {
        let content = UNMutableNotificationContent()
        content.title = notification.title
        content.body = notification.body
        content.sound = .default
        content.interruptionLevel = .timeSensitive
        content.userInfo = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
            PushNotificationKeys.deeplink: notification.deeplink
        ]

        let request = UNNotificationRequest(identifier: notification.identifier, content: content, trigger: nil)
        do {
            try await UNUserNotificationCenter.current().add(request)
        } catch {
            Logger.shared.warning("[chat-call] the notification of \(notification.identifier) did not show: \(error)")
        }
    }

    func remove(identifier: String) {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [identifier])
        center.removeDeliveredNotifications(withIdentifiers: [identifier])
    }
}
