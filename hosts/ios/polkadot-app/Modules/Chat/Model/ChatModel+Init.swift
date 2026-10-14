import Foundation

extension Chat.LocalModel {
    static func newChatWithContact(_ contact: Chat.Contact) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .person(contact),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: Date(),
            roomMetadata: nil
        )
    }

    static func newChatWithExtension(_ extensionId: ChatExtension.Id) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(extensionId),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: Date(),
            roomMetadata: nil
        )
    }

    static func newChatWithRoom(
        extensionId: ChatExtension.Id,
        roomId: String,
        roomMetadata: Chat.RoomMetadata
    ) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(extensionId, roomId: roomId),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: Date(),
            roomMetadata: roomMetadata
        )
    }
}
