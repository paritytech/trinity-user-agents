import Foundation
import SubstrateSdk

extension NSPredicate {
    static func chatRequestById(_ requestId: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatRequest.identifier), requestId)
    }
}
