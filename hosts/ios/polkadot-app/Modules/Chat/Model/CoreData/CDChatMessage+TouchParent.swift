import Foundation
import CoreData

extension CDChatMessage {
    func markModified() {
        modifiedAt = Int64(bitPattern: Date().toChatTimestamp())
    }

    func touchParent() {
        guard let parent = lastDisplayMessageChat else { return }
        let key = #keyPath(CDChat.lastDisplayMessage)
        parent.willChangeValue(forKey: key)
        parent.didChangeValue(forKey: key)
    }
}
