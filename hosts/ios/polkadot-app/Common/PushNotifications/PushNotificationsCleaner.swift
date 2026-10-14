import Foundation
import UserNotifications
import Operation_iOS
import StructuredConcurrency

protocol PushNotificationsCleaning: AnyObject {
    func cleanNotifications(
        for chatId: Chat.Id,
        messageIds: [Chat.MessageId]
    ) async throws
}

final class PushNotificationsCleaner {
    private let notificationService: UserNotificationServicing
    private let contactRepository: AnyDataProviderRepository<Chat.Contact>
    private let messageDecoder: ChatPushMessageDecoding

    init(
        notificationService: UserNotificationServicing,
        contactRepository: AnyDataProviderRepository<Chat.Contact>,
        messageDecoder: ChatPushMessageDecoding
    ) {
        self.notificationService = notificationService
        self.contactRepository = contactRepository
        self.messageDecoder = messageDecoder
    }
}

extension PushNotificationsCleaner: PushNotificationsCleaning {
    func cleanNotifications(
        for chatId: Chat.Id,
        messageIds: [Chat.MessageId]
    ) async throws {
        guard
            !messageIds.isEmpty,
            let accountId = chatId.accountId
        else {
            return
        }

        let contact = try await contactRepository
            .fetchOperation(by: { accountId.toHex() }, options: .init())
            .asyncExecute()

        guard let contact else {
            return
        }

        let notifications = await notificationService.deliveredNotifications()

        let uniqueMessageIds = Set(messageIds)

        let notificationIdentifiers: [String] = notifications.compactMap { notification in
            let userInfo = notification.request.content.userInfo

            guard
                let message = userInfo[PushNotificationKeys.message] as? String,
                let decodedMessage = try? messageDecoder.decodeMessage(message, for: contact)
            else {
                return nil
            }

            return uniqueMessageIds.contains(decodedMessage.messageId) ? notification.request.identifier : nil
        }

        guard !notificationIdentifiers.isEmpty else {
            return
        }
        notificationService.removeDeliveredNotifications(withIdentifiers: notificationIdentifiers)
    }
}
