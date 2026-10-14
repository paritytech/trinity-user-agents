import Foundation
import Foundation_iOS
import Operation_iOS
import MessageExchangeKit

protocol PushNotificationHandling {
    func handle(
        completion: @escaping (NotificationContentResult) -> Void
    )
}

protocol PushNotificationHandlerMaking {
    func createHandler(message: NotificationMessage) -> PushNotificationHandling
}

final class PushNotificationHandlerFactory: PushNotificationHandlerMaking {
    let messageDecoder: ChatPushMessageCoding
    let storage: any StorageFacadeProtocol

    init(
        messageDecoder: ChatPushMessageCoding,
        storage: any StorageFacadeProtocol
    ) {
        self.messageDecoder = messageDecoder
        self.storage = storage
    }

    func createHandler(message: NotificationMessage) -> PushNotificationHandling {
        switch message {
        case let .newMessage(pushId, text):
            let messageRepository = storage.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(PushNotificationChatMessageEntityMapper())
            )

            return NewMessageHandler(
                pushId: pushId,
                messageHex: text,
                contactsService: ContactsLocalStorageService(),
                messageDecoder: messageDecoder,
                messageRepository: AnyDataProviderRepository(messageRepository),
                unreadMessageCountService: UnreadMessageCountService()
            )
        }
    }
}

// MARK: Errors

enum PushNotificationsHandlerErrors: Error, Hashable {
    case noContact(String)
}
