import Foundation
import UserNotifications
import SubstrateSdk

enum PushForegroundVisibleScreen: Equatable {
    case chatList
    case chat(accountId: AccountId)
    /// A room of a chat extension or of a product. `roomId` is nil for the single room of a chat extension.
    case chatExtension(extensionId: ChatExtension.Id, roomId: String?)
    case other
}

protocol PushForegroundVisibilityReporting: AnyObject {
    func updateVisibleScreen(_ screen: PushForegroundVisibleScreen)
    func isChatVisible(for id: Chat.Id) -> Bool
}

protocol PushForegroundPresentationDeciding: AnyObject {
    func presentationOptions(for notification: UNNotification) -> UNNotificationPresentationOptions
}

final class PushForegroundPresentationController {
    private var visibleScreen: PushForegroundVisibleScreen = .other
}

extension PushForegroundPresentationController: PushForegroundVisibilityReporting {
    func updateVisibleScreen(_ screen: PushForegroundVisibleScreen) {
        visibleScreen = screen
    }

    func isChatVisible(for id: Chat.Id) -> Bool {
        switch (id, visibleScreen) {
        case let (.person(accountId), .chat(activeChatAccountId)):
            accountId == activeChatAccountId
        case let (.chatExtension(extensionId, roomId), .chatExtension(activeExtensionId, activeRoomId)):
            // A chat extension notification names no room: any room of the extension on screen shows it.
            extensionId == activeExtensionId && (roomId == nil || roomId == activeRoomId)
        default:
            false
        }
    }
}

extension PushForegroundPresentationController: PushForegroundPresentationDeciding {
    func presentationOptions(for notification: UNNotification) -> UNNotificationPresentationOptions {
        presentationOptions(for: notification.request.content)
    }

    /// The presentation of a notification with `content` while the app is in the foreground.
    func presentationOptions(for content: UNNotificationContent) -> UNNotificationPresentationOptions {
        guard !content.isFromBlockedContact else {
            return []
        }

        let shouldDisplay =
            if let productRoom = content.userInfo.productRoom {
                shouldDisplayProductRoomNotification(productId: productRoom.productId, roomId: productRoom.roomId)
            } else if content.isFromChat {
                shouldDisplayChatNotification(
                    userInfo: content.userInfo,
                    visibleScreen: visibleScreen
                )
            } else {
                true
            }

        return shouldDisplay ? [.banner] : []
    }
}

private extension PushForegroundPresentationController {
    func shouldDisplayChatNotification(
        userInfo: [AnyHashable: Any],
        visibleScreen: PushForegroundVisibleScreen
    ) -> Bool {
        switch visibleScreen {
        case .other:
            return true
        case .chatList:
            return false
        case let .chat(activeAccountId):
            guard let accountId = userInfo[PushNotificationKeys.accountId] as? Data else {
                return true
            }

            return accountId != activeAccountId
        case let .chatExtension(activeExtensionId, _):
            guard let extensionId = userInfo[PushNotificationKeys.chatExtensionId] as? ChatExtension.Id else {
                return true
            }

            return extensionId == activeExtensionId
        }
    }

    /// A notification of a room of the chat product shows as a person chat notification does: not while the chat
    /// list or that room is on screen, else as a banner.
    func shouldDisplayProductRoomNotification(productId: String, roomId: String) -> Bool {
        switch visibleScreen {
        case .chatList:
            false
        case let .chatExtension(extensionId, visibleRoomId):
            extensionId != productId || visibleRoomId != roomId
        case .chat,
             .other:
            true
        }
    }
}

extension UNNotificationContent {
    var isFromBlockedContact: Bool {
        userInfo[PushNotificationKeys.blockedContact] as? Bool == true
    }

    var isFromChat: Bool {
        let source = (userInfo[PushNotificationKeys.pushSource] as? Int)
            .flatMap { PushNotificationSource(rawValue: $0) }

        switch source {
        case .products:
            return false
        case .chat,
             .none:
            // as chat is main use case treat missing source as chat
            return true
        }
    }
}
