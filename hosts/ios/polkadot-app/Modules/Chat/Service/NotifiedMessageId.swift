import Foundation
import Operation_iOS

extension Chat {
    struct NotifiedMessageId: Hashable {
        let messageId: String

        init(messageId: String) {
            self.messageId = messageId
        }
    }
}

extension Chat.NotifiedMessageId: Operation_iOS.Identifiable {
    var identifier: String { messageId }
}
