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
        if case let .chatExtension(productId, roomId?) = chatId {
            await cleanProductRoomNotifications(productId: productId, roomId: roomId, messageIds: messageIds)
            return
        }

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

private extension PushNotificationsCleaner {
    /// The product marks every message of a room read with one `Seen`, so the notifications of the room go with
    /// the messages that the user saw.
    func cleanProductRoomNotifications(productId: String, roomId: String, messageIds: [Chat.MessageId]) async {
        guard !messageIds.isEmpty else {
            return
        }

        let notifications = await notificationService.deliveredNotifications()
        let identifiers = notifications.compactMap { notification -> String? in
            guard let room = notification.request.content.userInfo.productRoom,
                  room.productId == productId, room.roomId == roomId else {
                return nil
            }
            return notification.request.identifier
        }

        guard !identifiers.isEmpty else {
            return
        }
        notificationService.removeDeliveredNotifications(withIdentifiers: identifiers)
    }
}
