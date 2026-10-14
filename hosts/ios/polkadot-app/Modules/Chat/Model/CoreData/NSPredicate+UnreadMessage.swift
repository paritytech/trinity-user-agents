import Foundation

extension NSPredicate {
    static func unreadMessage(for messageId: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatUnreadMessage.messageId), messageId)
    }
}
