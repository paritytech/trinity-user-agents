import Foundation
import SubstrateSdk

extension NSPredicate {
    static func contact(for accountId: AccountId) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatContact.identifier), accountId.toHex())
    }
}
