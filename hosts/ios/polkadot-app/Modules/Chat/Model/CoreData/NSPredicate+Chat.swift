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

    /// The chats the chat list shows. While the chat product runs the chats, the native chat's person chats are not
    /// listed (HX2).
    static func listedChats(productChat: Bool) -> NSPredicate {
        guard productChat else { return chatWithNonBlockedContact() }
        let notPerson = NSPredicate(format: "%K != %d", #keyPath(CDChat.chatType), Chat.Id.ChatType.person.rawValue)
        return NSCompoundPredicate(andPredicateWithSubpredicates: [chatWithNonBlockedContact(), notPerson])
    }

    static func chatWithContact(for accountId: AccountId) -> NSPredicate {
        let chatContactId = Chat.Id.person(accountId)

        return NSPredicate(format: "%K == %@", #keyPath(CDChat.identifier), chatContactId.rawRepresentation)
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

    static func chatWithActiveIncomingRequests() -> NSPredicate {
        let attachedRequestPredicate = NSPredicate(format: "%K != nil", #keyPath(CDChat.contact.chatRequest))

        let personTypePredicate = NSPredicate(
            format: "%K == %d",
            #keyPath(CDChat.chatType),
            Chat.Id.ChatType.person.rawValue
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            personTypePredicate,
            attachedRequestPredicate
        ])
    }
}
