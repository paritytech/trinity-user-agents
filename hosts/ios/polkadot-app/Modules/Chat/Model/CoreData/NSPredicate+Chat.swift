import Foundation
import SubstrateSdk

extension NSPredicate {
    static func chat(for identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChat.identifier), identifier)
    }

    static func chatWithNonBlockedContact() -> NSPredicate {
        NSPredicate(
            format: "%K == NO OR %K == nil",
            #keyPath(CDChat.contact.isBlocked),
            #keyPath(CDChat.contact.isBlocked)
        )
    }

    /// The chats the chat list shows. The chat product runs the person chats, so the person chats the native chat
    /// left behind (kept only for the migration to the product) are not listed (HX2).
    static func listedChats() -> NSPredicate {
        let notPerson = NSPredicate(format: "%K != %d", #keyPath(CDChat.chatType), Chat.Id.ChatType.person.rawValue)
        return NSCompoundPredicate(andPredicateWithSubpredicates: [chatWithNonBlockedContact(), notPerson])
    }

    static func roomChatsForExtension(_ extensionId: ChatExtension.Id) -> NSPredicate {
        let extensionType = NSPredicate(
            format: "%K == %d",
            #keyPath(CDChat.chatType),
            Chat.Id.ChatType.chatExtension.rawValue
        )

        let extensionMatch = NSPredicate(
            format: "%K == %@",
            #keyPath(CDChat.chatTypeContext),
            extensionId
        )

        let hasRoom = NSPredicate(
            format: "%K != nil",
            #keyPath(CDChat.roomMetadata)
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [extensionType, extensionMatch, hasRoom])
    }
}
