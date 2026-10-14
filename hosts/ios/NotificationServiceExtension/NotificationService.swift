import UserNotifications
import MessageExchangeKit

class NotificationService: UNNotificationServiceExtension {
    typealias ContentHandler = (UNNotificationContent) -> Void

    var contentHandler: ContentHandler?
    var bestAttemptContent: UNNotificationContent?
    var handler: PushNotificationHandling?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let messageDecoder = ChatPushMessageCoder(encryptionManager: ChatEncryptionManager())

        bestAttemptContent = NotificationContentResult
            .createBestAttemptResult()
            .toFallbackUserNotificationContent(with: request.content)

        self.contentHandler = contentHandler

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
