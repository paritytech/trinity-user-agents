import UserNotifications
import MessageExchangeKit

class NotificationService: UNNotificationServiceExtension {
    typealias ContentHandler = (UNNotificationContent) -> Void

    var contentHandler: ContentHandler?
    var bestAttemptContent: UNNotificationContent?
    var handler: PushNotificationHandling?
    var productHandler: ProductPushHandler?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let messageDecoder = ChatPushMessageCoder(encryptionManager: ChatEncryptionManager())

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

        do {
            let message = try NotificationMessage(userInfo: request.content.userInfo)

            handler = PushNotificationHandlerFactory(
                messageDecoder: messageDecoder,
                storage: UserDataStorageFacade.shared
            )
            .createHandler(message: message)

            handler?.handle { content in
                let notificationContent = content.toUserNotificationContent(with: request.content)
                contentHandler(notificationContent)
            }
        } catch {
            let unsupported = NotificationContentResult.createUnsupportedResult()
            let notificationContent = unsupported.toUserNotificationContent(with: request.content)
            contentHandler(notificationContent)
        }
    }

    override func serviceExtensionTimeWillExpire() {
        // Called just before the extension will be terminated by the system.
        if let contentHandler, let bestAttemptContent {
            contentHandler(bestAttemptContent)
        }
    }
}
