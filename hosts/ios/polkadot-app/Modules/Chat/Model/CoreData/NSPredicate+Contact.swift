import Foundation
import SubstrateSdk

extension NSPredicate {
    static func contact(for accountId: AccountId) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatContact.identifier), accountId.toHex())
    }

    static func blockedContacts() -> NSPredicate {
        NSPredicate(format: "%K == YES", #keyPath(CDChatContact.isBlocked))
    }

    static func pendingDevicesFanOutContacts() -> NSPredicate {
        NSPredicate(format: "%K == YES", #keyPath(CDChatContact.pendingDevicesFanOut))
    }

    static func contact(accountId: AccountId) -> NSPredicate {
        NSCompoundPredicate(andPredicateWithSubpredicates: [contact(for: accountId), isContact()])
    }

    static func contact(beginsWith prefix: String) -> NSPredicate {
        let begins = NSPredicate(
            format: "%K BEGINSWITH[c] %@",
            #keyPath(CDChatContact.username),
            prefix
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [begins, isContact()])
    }

    static func isContact() -> NSPredicate {
        NSPredicate(format: "%K == nil", #keyPath(CDChatContact.chatRequest))
    }
}
