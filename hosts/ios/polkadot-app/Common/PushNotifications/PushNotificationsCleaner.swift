import Foundation
import UserNotifications

protocol PushNotificationsCleaning: AnyObject {
    func cleanNotifications(
        for chatId: Chat.Id,
        messageIds: [Chat.MessageId]
    ) async throws
}

/// Removes the delivered notifications of the messages the user saw. Only the rooms of products notify (the chat
/// product among them), so only their notifications are cleaned.
final class PushNotificationsCleaner {
    private let notificationService: UserNotificationServicing

    init(notificationService: UserNotificationServicing) {
        self.notificationService = notificationService
    }
}

extension PushNotificationsCleaner: PushNotificationsCleaning {
    func cleanNotifications(
        for chatId: Chat.Id,
        messageIds: [Chat.MessageId]
    ) async throws {
        guard case let .chatExtension(productId, roomId?) = chatId else {
            return
        }

        await cleanProductRoomNotifications(productId: productId, roomId: roomId, messageIds: messageIds)
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
