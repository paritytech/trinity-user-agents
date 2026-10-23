import Foundation
import SubstrateSdk

extension NSPredicate {
    static func localMessages(from chatId: Chat.Id) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.chat.identifier),
            chatId.rawRepresentation
        )
    }

    static func messages(in chat: CDChat) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.chat), chat)
    }

    static func messages(withIds ids: [String]) -> NSPredicate {
        NSPredicate(
            format: "%K IN %@",
            #keyPath(CDChatMessage.messageId),
            ids
        )
    }

    /// Messages compacted into a blob are excluded: their delivery is tracked
    /// through the compacted parent, so they must not re-enter sync logic.
    static func byStatus(_ status: Chat.LocalMessage.Status) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.status),
            status.rawValue
        )
    }

    static func messageByChatType(_ chatType: Chat.Id.ChatType) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.chat.chatType),
            chatType.rawValue
        )
    }

    /// A claimed or failed transfer never re-enters the monitor.
    static func messageByContentType(_ contentType: Chat.LocalMessage.Content.ContentType) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.contentType),
            contentType.rawValue
        )
    }

    static func messageByContentKey(_ contentKey: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.contentKey),
            contentKey
        )
    }

    static func chatMessage(with remoteMessageId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.messageId),
            remoteMessageId
        )
    }

    static func siblingGroupMessages(
        groupingId: String,
        excluding entity: CDChatMessage
    ) -> NSPredicate {
        NSPredicate(
            format: "%K == %@ AND SELF != %@",
            #keyPath(CDChatMessage.groupingId),
            groupingId,
            entity
        )
    }

    static func editHistory(for messageId: String) -> NSPredicate {
        let originalMessagePredicate = NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.messageId),
            messageId
        )

        let editedContentType = Int16(Chat.LocalMessage.Content.ContentType.edited.rawValue)
        let editedMessagesPredicate = NSPredicate(
            format: "%K == %d AND %K == %@",
            #keyPath(CDChatMessage.contentType),
            editedContentType,
            #keyPath(CDChatMessage.contentKey),
            messageId
        )

        return NSCompoundPredicate(orPredicateWithSubpredicates: [originalMessagePredicate, editedMessagesPredicate])
    }
}
