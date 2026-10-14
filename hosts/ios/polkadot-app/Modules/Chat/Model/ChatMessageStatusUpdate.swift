import Foundation
import Operation_iOS

extension Chat {
    struct ChatMessageStatusUpdate: Identifiable {
        let messageId: String
        let status: Chat.LocalMessage.Status

        var identifier: String { messageId }

        func replacingStatus(_ newStatus: Chat.LocalMessage.Status) -> Self {
            ChatMessageStatusUpdate(
                messageId: messageId,
                status: newStatus
            )
        }
    }
}
