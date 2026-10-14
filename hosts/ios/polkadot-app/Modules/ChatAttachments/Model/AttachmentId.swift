import Foundation

struct AttachmentId: Hashable {
    let messageId: Chat.MessageId
    let fileId: String

    var stringValue: String {
        [messageId, fileId].joined(with: .colon)
    }
}
