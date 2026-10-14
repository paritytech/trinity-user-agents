import Foundation

extension NSPredicate {
    static func chatExtensionHistoryForChat(_ chatId: Chat.Id) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatExtensionHistory.chatId), chatId.rawRepresentation)
    }
}
