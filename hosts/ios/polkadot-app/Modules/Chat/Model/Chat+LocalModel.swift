import Foundation
import SubstrateSdk

extension Chat {
    enum Peer: Equatable {
        case person(Contact)
        case chatExtension(ChatExtension.Id, roomId: String?)

        var chatId: Chat.Id {
            switch self {
            case let .person(contact):
                .person(contact.accountId)
            case let .chatExtension(extensionId, roomId):
                .chatExtension(extensionId, roomId: roomId)
            }
        }
    }

    // swiftlint:disable:next type_name
    enum Id: Hashable {
        case person(AccountId)
        case chatExtension(ChatExtension.Id, roomId: String?)

        var isPerson: Bool {
            switch self {
            case .person:
                true
            case .chatExtension:
                false
            }
        }

        var accountId: AccountId? {
            switch self {
            case let .person(accountId):
                accountId
            case .chatExtension:
                nil
            }
        }

        var extensionId: ChatExtension.Id? {
            switch self {
            case .person:
                nil
            case let .chatExtension(extId, _):
                extId
            }
        }

        var roomId: String? {
            switch self {
            case .person:
                nil
            case let .chatExtension(_, roomId):
                roomId
            }
        }
    }

    struct LocalModel {
        let peer: Chat.Peer
        let message: Chat.LocalMessage?
        let unreadDisplayMessageCount: Int
        let hasIncomingReaction: Bool
        let createdAt: Date?
        let roomMetadata: Chat.RoomMetadata?
        var roomFooter: Chat.RoomFooter?

        var chatId: Chat.Id {
            peer.chatId
        }
    }
}

extension Chat.Peer {
    var contact: Chat.Contact? {
        guard case let .person(contact) = self else {
            return nil
        }
        return contact
    }

    static func chatExtension(_ extensionId: ChatExtension.Id) -> Chat.Peer {
        .chatExtension(extensionId, roomId: nil)
    }
}

extension Chat.Id {
    static func chatExtension(_ extensionId: ChatExtension.Id) -> Chat.Id {
        .chatExtension(extensionId, roomId: nil)
    }
}
