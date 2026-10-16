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
    private let logger: LoggerProtocol

    init(logger: LoggerProtocol = Logger.shared) {
        self.logger = logger
    }
}

extension PushForegroundPresentationController: PushForegroundVisibilityReporting {
    func updateVisibleScreen(_ screen: PushForegroundVisibleScreen) {
        visibleScreen = screen
    }

    func isChatVisible(for id: Chat.Id) -> Bool {
        switch (id, visibleScreen) {
        case let (.person(accountId), .chat(activeChatAccountId)):
            accountId == activeChatAccountId
        case let (.chatExtension(extensionId, _), .chatExtension(activeExtensionId, _)):
            extensionId == activeExtensionId
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
            if let productRoom = content.productRoomLink {
                shouldDisplayProductRoomNotification(room: productRoom)
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

    /// A notification of a product that opens a room of the product shows as a person chat notification does: not
    /// while the chat list or that room is on screen, else as a banner.
    func shouldDisplayProductRoomNotification(room: ChatRoomLink) -> Bool {
        let shouldDisplay =
            switch visibleScreen {
            case .chatList:
                false
            case let .chatExtension(extensionId, roomId):
                extensionId != room.productId || roomId != room.roomId
            case .chat,
                 .other:
                true
            }
        logger.debug(
            "Foreground notification of \(room.productId) room \(room.roomId): \(shouldDisplay ? "banner" : "hidden")"
        )
        return shouldDisplay
    }
}

extension UNNotificationContent {
    /// The room that a notification of a product opens, when its deeplink is a room link of the same product.
    var productRoomLink: ChatRoomLink? {
        guard
            let productId = userInfo[PushNotificationKeys.productId] as? String,
            let deeplink = userInfo[PushNotificationKeys.deeplink] as? String,
            let link = ChatRoomLink(deeplink),
            link.productId == productId
        else {
            return nil
        }
        return link
    }

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
