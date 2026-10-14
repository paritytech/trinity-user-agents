import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk

final class PeerChatPushRouteHandler: ChatPushRouteHandler {
    private let contactsStorage: ContactsLocalStorageServicing

    init(
        contactsStorage: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        moduleNavigator: ModuleNavigating,
        logger: LoggerProtocol = Logger.shared,
        visibilityReporter: PushForegroundVisibilityReporting?
    ) {
        self.contactsStorage = contactsStorage

        super.init(
            moduleNavigator: moduleNavigator,
            logger: logger,
            visibilityReporter: visibilityReporter
        )
    }

    override func handle(_ route: PushNavigationRoute) -> Bool {
        guard case let .contactChat(userInfo) = route else {
            return false
        }

        resolveContact(userInfo)

        return true
    }
}

private extension PeerChatPushRouteHandler {
    func resolveContact(_ userInfo: [AnyHashable: Any]) {
        if let accountId = userInfo[PushNotificationKeys.accountId] as? AccountId {
            Task { @MainActor [weak self] in
                self?.handle(chatId: .person(accountId))
            }
        } else if let pushId = userInfo[PushNotificationKeys.pushId] as? String {
            Task { [weak self] in
                guard let contact = try await self?.contactsStorage.getContact(byPushId: pushId).asyncExecute() else {
                    self?.logger.warning("No contact found for push id \(pushId)")
                    return
                }

                await self?.handle(chatId: .person(contact.accountId))
            }
        }
    }

    @MainActor
    func handle(chatId: Chat.Id) {
        guard visibilityReporter?.isChatVisible(for: chatId) != true else {
            return
        }

        moduleNavigator.openChat(chatId)
    }
}
