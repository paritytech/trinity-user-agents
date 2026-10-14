import Foundation
import CoreData

extension CDChatRequest {
    func touchParent() {
        guard let parent = contact?.chat else { return }
        let key = #keyPath(CDChat.contact)
        parent.willChangeValue(forKey: key)
        parent.didChangeValue(forKey: key)
    }
}
