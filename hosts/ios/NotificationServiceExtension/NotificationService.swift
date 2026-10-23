import SDKLogger
import UserNotifications

class NotificationService: UNNotificationServiceExtension {
    typealias ContentHandler = (UNNotificationContent) -> Void

    var contentHandler: ContentHandler?
    var bestAttemptContent: UNNotificationContent?
    var productHandler: ProductPushHandler?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        bestAttemptContent = NotificationContentResult
            .createBestAttemptResult()
            .toFallbackUserNotificationContent(with: request.content)

        self.contentHandler = contentHandler

        if let productHandler = ProductPushHandler.make(
            userInfo: request.content.userInfo,
            storage: UserDataStorageFacade.shared
        ) {
            self.productHandler = productHandler
            // The notification that shows when the time runs out opens the room too.
            if let fallback = bestAttemptContent?.mutableCopy() as? UNMutableNotificationContent {
                fallback.userInfo = productHandler.userInfo(of: fallback)
                bestAttemptContent = fallback
            }
            productHandler.handle { content in
                // A push that carries nothing to show shows nothing, as the native chat shows no notification for
                // such a message.
                guard let content else {
                    contentHandler(UNNotificationContent())
                    return
                }

                let notificationContent = content.toUserNotificationContent(with: request.content)
                let productContent = notificationContent.mutableCopy() as? UNMutableNotificationContent
                productContent?.userInfo = productHandler.userInfo(of: notificationContent)
                contentHandler(productContent ?? notificationContent)
            }
            return
        }

        // Only the products notify, the chat product among them (HX2). A push no product claims has no route to
        // decode it by: it shows as unsupported, since an extension without the filtering entitlement cannot drop it.
        Logger.shared.warning("A push that no product claims shows as unsupported")
        let unsupported = NotificationContentResult.createUnsupportedResult()
        let notificationContent = unsupported.toUserNotificationContent(with: request.content)
        contentHandler(notificationContent)
    }

    override func serviceExtensionTimeWillExpire() {
        // Called just before the extension will be terminated by the system.
        if let contentHandler, let bestAttemptContent {
            contentHandler(bestAttemptContent)
        }
    }
}
