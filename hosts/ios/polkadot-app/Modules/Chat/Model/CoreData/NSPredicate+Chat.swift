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
