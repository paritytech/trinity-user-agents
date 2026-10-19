import Foundation
import UserNotifications

/// The notification of a push of a product room (`push.notify`): its push id names a room of this device, and its
/// message opens with the key of that room. A tap opens the room.
enum ProductPushContent {
    /// The content to show, or nil when the push is not of a product room. A push that does not open shows
    /// `fallback`.
    static func make(
        userInfo: [AnyHashable: Any],
        original: UNNotificationContent,
        fallback: (title: String, body: String),
        routes: ProductPushRoutes = ProductPushRoutes()
    ) -> UNNotificationContent? {
        guard
            let pushId = userInfo[PushNotificationKeys.pushId] as? String,
            let message = userInfo[PushNotificationKeys.message] as? String,
            let route = routes.route(pushId: pushId)
        else {
            return nil
        }

        let content = original.mutableCopy() as? UNMutableNotificationContent ?? UNMutableNotificationContent()
        let notification = ProductPushNotification.open(messageHex: message, key: route.key)
        content.title = notification?.title ?? fallback.title
        content.body = notification?.body ?? fallback.body
        content.sound = .default
        content.threadIdentifier = "\(route.productId)/\(route.roomId)"
        content.userInfo = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
            PushNotificationKeys.productId: route.productId,
            PushNotificationKeys.deeplink: "polkadot://\(route.productId)/-/chat/\(route.roomId)"
        ]
        return content
    }
}
