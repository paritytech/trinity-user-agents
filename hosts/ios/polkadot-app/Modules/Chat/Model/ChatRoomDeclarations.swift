import Foundation
import SubstrateSdk

extension Chat {
    /// What a product declares for one of its rooms (02-DESIGN §8.2). The host keeps it in
    /// `CDChatRoomMetadata`, so the chat list can draw the room without the product worker.
    ///
    /// The store keeps the configuration as the SCALE bytes of its Chat Extension v2 wire value.
    /// A nil value means that the product declared nothing.
    struct RoomDeclarations: Equatable {
        let config: RoomConfig?
        /// Goes up by one at each configuration that the product sets, so an open room reloads its policy.
        let configGeneration: Int64
        let preview: RoomPreview
        /// The optional features of the Chat Extension v2 additions. A feature that this build does not serve
        /// stays here, and the host ignores it.
        var features: RoomFeatures = .hostDefault
        /// The person the room is a conversation with. The host draws such a room as its own person chat.
        var contact: RoomContact?

        /// The configuration that applies: the declared one, or the host default.
        var effectiveConfig: RoomConfig {
            config ?? .hostDefault
        }
    }

    /// The person that a room of the chat product is a conversation with (`ChatRoomContact`).
    struct RoomContact: Equatable {
        /// Where the conversation stands (`ChatContactState`). The raw value is the stored form.
        enum State: UInt8, Equatable {
            /// No request yet: the input is the invite composer. The chat list does not show the room.
            case invite = 0
            /// The request of the user waits for the contact.
            case outgoing = 1
            /// The request of the contact waits for the user.
            case incoming = 2
            case established = 3
            /// The user blocked the contact. The chat list does not show the room.
            case blocked = 4
            /// The contact left the chat.
            case peerLeft = 5
        }

        let account: AccountId
        let username: String?
        let state: State
    }

    struct RoomPreview: Equatable {
        /// The sort key in milliseconds since the Unix epoch. Nil uses the time of the last message.
        let timestamp: Int64?
        let badge: RoomBadge
        /// The newest message that the room shows, as the room screen draws it. Nil draws the stored last message.
        let lastMessage: RoomPreviewMessage?
        /// The contact reacted to a message, and the user did not see the reaction yet.
        let unseenReaction: Bool

        /// The preview of a room whose product set none: the host draws the last message.
        static let hostDefault = RoomPreview(
            timestamp: nil,
            badge: .hostDefault,
            lastMessage: nil,
            unseenReaction: false
        )
    }

    /// The newest message of a room that the product keeps, which the chat list draws as the native preview line.
    struct RoomPreviewMessage: Equatable {
        let messageId: MessageId
        let status: LocalMessage.Status
        let timestamp: Timestamp
        let content: LocalMessage.Content
        /// The edits and the call signals of the message, which the native preview line reads.
        let relatedMessages: [RelatedLocalMessage]

        /// The last message of the chat list. The native chat keeps the end of a call, or else its answer, as the
        /// last message of its chat, so the list shows and orders a call by the time of that message.
        func localMessage(chatId: Chat.Id) -> LocalMessage {
            guard let signal = newestCallSignal else {
                return localMessage(chatId: chatId, from: asRelated, relatedMessages: relatedMessages)
            }

            let others = relatedMessages.filter { $0.messageId != signal.messageId }
            return localMessage(chatId: chatId, from: signal, relatedMessages: [asRelated] + others)
        }
    }

    enum RoomBadge: Equatable {
        /// The host counts the unread messages.
        case hostDefault
        case none
        /// Zero shows no badge.
        case unreadCount(Int32)
    }

    /// The parts of a room that the product serves itself (`ChatRoomConfig`).
    struct RoomConfig: Equatable {
        /// The product keeps the messages, and the host keeps only a page cache.
        let productSource: Bool
        /// The host offers "who reacted" and asks the product for it.
        let reactionDetails: Bool
        /// The product wants a `Seen` event for the messages that the user saw.
        let reportSeen: Bool

        /// The configuration of a room that declares nothing.
        static let hostDefault = RoomConfig(productSource: false, reactionDetails: false, reportSeen: false)
    }
}

extension Chat {
    /// Replaces the name of a room and keeps its v1 icon.
    struct RoomNameUpdate {
        let chatId: Chat.Id
        let name: String
    }

    struct RoomConfigUpdate {
        let chatId: Chat.Id
        let config: RoomConfig
    }

    struct RoomPreviewUpdate {
        let chatId: Chat.Id
        let preview: RoomPreview
    }

    /// Replaces the contact of a room.
    struct RoomContactUpdate {
        let chatId: Chat.Id
        let contact: RoomContact
    }

    /// The declarations of a stored room, as the read mapper gives them.
    struct StoredRoomDeclarations {
        let chatId: String
        let declarations: RoomDeclarations
    }
}

extension Chat.RoomContact {
    /// The chat of the account in the native chat. Its color seed colors the avatar of the contact.
    var personChatId: Chat.Id {
        .person(account)
    }

    /// The name of the contact: the username, else the name that the product gave the room.
    func name(fallback: String) -> String {
        guard let username, !username.isEmpty else {
            return fallback
        }

        return username
    }

    /// The chat list shows the room: an invite has no request yet, and the user blocked a blocked contact.
    var isListed: Bool {
        switch state {
        case .invite,
             .blocked:
            false
        case .outgoing,
             .incoming,
             .established,
             .peerLeft:
            true
        }
    }
}

private extension Chat.RoomPreviewMessage {
    var asRelated: Chat.RelatedLocalMessage {
        Chat.RelatedLocalMessage(messageId: messageId, timestamp: timestamp, content: content, status: status)
    }

    /// The end of a call, or else its answer: the native chat stores the signals of a call in that order.
    var newestCallSignal: Chat.RelatedLocalMessage? {
        guard case .call(.offer) = content else {
            return nil
        }

        let closed = relatedMessages.first { member in
            if case .call(.closed) = member.content { return true }
            return false
        }
        let answer = relatedMessages.first { member in
            if case .call(.answer) = member.content { return true }
            return false
        }

        return closed ?? answer
    }

    func localMessage(
        chatId: Chat.Id,
        from message: Chat.RelatedLocalMessage,
        relatedMessages: [Chat.RelatedLocalMessage]
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: message.messageId,
            chatId: chatId,
            origin: message.status.isOutgoing ? .user : .chatExtension(chatId.extensionId ?? ""),
            creationSource: .localDevice,
            status: message.status,
            timestamp: message.timestamp,
            content: message.content,
            reactions: [],
            compactionId: nil,
            relatedMessages: relatedMessages
        )
    }
}
