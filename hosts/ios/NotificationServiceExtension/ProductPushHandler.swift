import BigInt
import ChainRegistry
import Foundation
import Foundation_iOS
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import TrUAPIHost
import UserNotifications

/// The notification of a push of the chat product, as the native chat shows the notification of a contact: the
/// name of the contact, the text of the native chat for the kind of the message, the unread count of the app as its
/// badge, and no text for a contact that the user blocked. A tap opens the room of the contact.
///
/// The core opens the push with the key of the route and says what kind of message it carries (`decodeChatPush`).
final class ProductPushHandler {
    private let route: ProductPushRoute
    private let messageHex: String
    private let storage: StorageFacadeProtocol
    private let unreadMessageCountService: UnreadMessageCountService

    init(
        route: ProductPushRoute,
        messageHex: String,
        storage: StorageFacadeProtocol,
        unreadMessageCountService: UnreadMessageCountService
    ) {
        self.route = route
        self.messageHex = messageHex
        self.storage = storage
        self.unreadMessageCountService = unreadMessageCountService
    }

    /// The handler of a push whose push id names a room of the chat product, or nil for any other push.
    static func make(
        userInfo: [AnyHashable: Any],
        storage: StorageFacadeProtocol,
        routes: ProductPushRoutes = ProductPushRoutes()
    ) -> ProductPushHandler? {
        guard
            let pushId = userInfo[PushNotificationKeys.pushId] as? String,
            let message = userInfo[PushNotificationKeys.message] as? String,
            let route = routes.route(pushId: pushId)
        else {
            return nil
        }

        return ProductPushHandler(
            route: route,
            messageHex: message,
            storage: storage,
            unreadMessageCountService: UnreadMessageCountService(databaseService: storage.databaseService)
        )
    }

    /// Gives the content of the notification, or nil when the push carries nothing to show.
    func handle(completion: @escaping (NotificationContentResult?) -> Void) {
        Task {
            guard await !isContactBlocked() else {
                let badgeCount = await calculateBadgeCount()
                completion(.createBlockedContactResult(badgeCount: badgeCount))
                return
            }

            guard
                let message = try? Data(hexString: messageHex),
                let preview = decodeChatPush(key: route.key, message: message)
            else {
                completion(nil)
                return
            }

            // The native extension saves each pushed message, so the badge counts every push until the app reads
            // the room. The room of the product counts one more unread message for each push that the native chat
            // counts.
            if preview.countsAsUnread {
                await countUnreadMessage()
            }
            let badgeCount = await calculateBadgeCount()

            do {
                let username = route.chatTitle ?? String(localized: .commonTitle)
                let result = try await ProductPushWording(username: username).contentResult(for: preview)
                completion(result.withBadgeCount(badgeCount))
            } catch {
                completion(.createUnsupportedResult(badgeCount: badgeCount))
            }
        }
    }

    /// The notification opens the room of the contact, not a chat of the native chat.
    func userInfo(of content: UNNotificationContent) -> [AnyHashable: Any] {
        var userInfo: [AnyHashable: Any] = [
            PushNotificationKeys.pushSource: PushNotificationSource.chat.rawValue,
            PushNotificationKeys.productId: route.productId,
            PushNotificationKeys.productRoomId: route.roomId
        ]
        if content.userInfo[PushNotificationKeys.blockedContact] as? Bool == true {
            userInfo[PushNotificationKeys.blockedContact] = true
        }
        return userInfo
    }
}

private extension ProductPushHandler {
    func isContactBlocked() async -> Bool {
        let chatId = Chat.Id.chatExtension(route.productId, roomId: route.roomId)
        let contact: Chat.RoomContact?? = try? await storage.databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return Chat.RoomDeclarations(entity: chat?.roomMetadata).contact
        }
        return contact??.state == .blocked
    }

    /// Adds one to the unread count of the room, until the product sets the count again.
    func countUnreadMessage() async {
        let chatId = Chat.Id.chatExtension(route.productId, roomId: route.roomId)
        try? await storage.databaseService.performWrite { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            guard let metadata = chat?.roomMetadata else { return }

            metadata.badgeKind = Self.unreadCountBadgeKind
            metadata.badgeCount += 1
        }
    }

    /// The stored kind of an `UnreadCount` badge (`ChatRoomDeclarationMappers.swift`).
    static let unreadCountBadgeKind: Int16 = 2

    func calculateBadgeCount() async -> Int? {
        try? await unreadMessageCountService.totalUnreadBadgeMessageCount()
    }
}

private extension ChatPushPreview {
    /// The native badge does not count reactions (`UnreadMessageCountService.badgeExcludedContentTypes`), and an
    /// edit counts with the message that it edits.
    var countsAsUnread: Bool {
        switch self {
        case .call:
            true
        case let .message(_, content):
            switch content {
            case .reaction,
                 .reactionRemoved,
                 .edited:
                false
            case .text,
                 .richText,
                 .reply,
                 .contactAdded,
                 .requestAccepted,
                 .leftChat,
                 .payment,
                 .unsupported:
                true
            }
        }
    }
}

/// The title and the text of the notification of each kind of message, with the strings of the native chat
/// (`NewMessageHandler`).
struct ProductPushWording {
    let username: String

    func contentResult(for preview: ChatPushPreview) async throws -> NotificationContentResult {
        switch preview {
        case let .call(_, video):
            NotificationContentResult(
                title: username,
                body: String(localized: video ? .incomingVideoCall : .incomingAudioCall)
            )
        case let .message(_, content):
            try await contentResult(for: content)
        }
    }
}

private extension ProductPushWording {
    // swiftlint:disable:next cyclomatic_complexity
    func contentResult(for content: ChatPushContent) async throws -> NotificationContentResult {
        switch content {
        case let .text(text):
            NotificationContentResult(title: username, body: text)
        case let .payment(amount, legacy):
            try await NotificationContentResult(
                title: username,
                body: makeBody(amount: Balance(amount) ?? 0, legacy: legacy, displayInfo: assetDisplayInfo())
            )
        case .contactAdded:
            NotificationContentResult(title: username, body: String(localized: .contactAddedYou(username: username)))
        case let .reply(text, _):
            NotificationContentResult(title: username, body: text ?? "")
        case let .reaction(emoji):
            NotificationContentResult(title: username, body: String(localized: .reactedWithEmoji(emoji: emoji)))
        case .reactionRemoved:
            NotificationContentResult(title: username, body: String(localized: .reactionRemoved))
        case let .edited(text):
            NotificationContentResult(title: username, body: String(localized: .messageEdited(text: text ?? "")))
        case .leftChat:
            NotificationContentResult(title: username, body: String(localized: .peerLeftChat(username: username)))
        case .requestAccepted:
            NotificationContentResult(title: username, body: String(localized: .chatRequestAccepted))
        case let .richText(text, _):
            NotificationContentResult(title: username, body: makeBody(richText: text))
        case .unsupported:
            NotificationContentResult(title: username, body: String(localized: .unsupportedContent))
        }
    }

    func assetDisplayInfo() async throws -> AssetBalanceDisplayInfo {
        let mapper = ChainModelMapper()
        let repository = SubstrateDataStorageFacade.shared
            .createRepository(mapper: AnyCoreDataMapper(mapper))
        let mainAsset = AppConfig.Assets.mainAsset
        let chain = try await repository.fetchOperation(
            by: { mainAsset.chainId },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()

        let asset = chain?.assets.first(where: { $0.assetId == mainAsset.assetId })
        return asset?.digitalDollarDisplayInfo ?? .usd
    }

    /// A legacy transfer shows the amount with the token formatter, a coinage payment with the transfer formatter.
    func makeBody(amount: Balance, legacy: Bool, displayInfo: AssetBalanceDisplayInfo) -> String {
        let formatterFactory = AssetBalanceFormatterFactory()
        let amountString: String
        if legacy {
            let formatter = formatterFactory.createTokenFormatter(for: displayInfo).value(for: .current)
            amountString = formatter.stringFromDecimal(amount.decimal(assetInfo: displayInfo)) ?? ""
        } else {
            amountString = TransferAmountViewModelFactory(
                targetAssetInfo: displayInfo,
                formatterFactory: formatterFactory
            )
            .amount(from: amount)
        }
        return String(localized: .contactSentYouAsset(username: username)) + " \(amountString)"
    }

    func makeBody(richText text: String?) -> String {
        let displayText = text ?? ""

        guard displayText.isEmpty else {
            return displayText
        }

        return String(localized: .mediaAttachment)
    }
}
