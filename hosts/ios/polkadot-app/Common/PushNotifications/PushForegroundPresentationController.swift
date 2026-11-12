import Foundation
import UserNotifications
import SubstrateSdk

enum PushForegroundVisibleScreen: Equatable {
    case chatList
    case chat(accountId: AccountId)
    case chatExtension(extensionId: ChatExtension.Id)
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
        case let (.chatExtension(extensionId, _), .chatExtension(activeExtensionId)):
            extensionId == activeExtensionId
        default:
            false
        }
    }
}

extension PushForegroundPresentationController: PushForegroundPresentationDeciding {
    func presentationOptions(for notification: UNNotification) -> UNNotificationPresentationOptions {
        guard !notification.request.content.isFromBlockedContact else {
            return []
        }

        let shouldDisplay =
            if notification.request.content.isFromChat {
                shouldDisplayChatNotification(
                    userInfo: notification.request.content.userInfo,
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
        case let .chatExtension(activeExtensionId):
            guard let extensionId = userInfo[PushNotificationKeys.chatExtensionId] as? ChatExtension.Id else {
                return true
            }

            return extensionId == activeExtensionId
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
