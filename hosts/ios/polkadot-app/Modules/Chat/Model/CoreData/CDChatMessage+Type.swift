import Foundation

extension CDChatMessage {
    var isSystem: Bool {
        let type = Chat.LocalMessage.Content.ContentType(rawValue: UInt8(contentType))
        return type?.isSystem ?? false
    }

    var isReaction: Bool {
        let type = Chat.LocalMessage.Content.ContentType(rawValue: UInt8(contentType))
        return type?.isReaction ?? false
    }
}
