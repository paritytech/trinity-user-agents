import Foundation
import CoreData

extension NSPredicate {
    static func reactionsFor(messageId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessageReaction.message.messageId),
            messageId
        )
    }

    static func reactionsFor(chatId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessageReaction.message.chat.identifier),
            chatId
        )
    }

    static func reaction(
        messageId: String,
        emoji: String,
        originType: Int16,
        originKey: String?
    ) -> NSPredicate {
        if let originKey {
            NSPredicate(
                format: "%K == %@ AND %K == %@ AND %K == %d AND %K == %@",
                #keyPath(CDChatMessageReaction.message.messageId),
                messageId,
                #keyPath(CDChatMessageReaction.emoji),
                emoji,
                #keyPath(CDChatMessageReaction.originType),
                originType,
                #keyPath(CDChatMessageReaction.originKey),
                originKey
            )
        } else {
            NSPredicate(
                format: "%K == %@ AND %K == %@ AND %K == %d AND %K == nil",
                #keyPath(CDChatMessageReaction.message.messageId),
                messageId,
                #keyPath(CDChatMessageReaction.emoji),
                emoji,
                #keyPath(CDChatMessageReaction.originType),
                originType,
                #keyPath(CDChatMessageReaction.originKey)
            )
        }
    }
}
