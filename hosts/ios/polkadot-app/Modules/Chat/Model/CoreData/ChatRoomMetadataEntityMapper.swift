import Foundation
import CoreData

final class ChatRoomMetadataEntityMapper {
    func transform(entity: CDChatRoomMetadata) throws -> Chat.RoomMetadata {
        guard let chatRelativeId = entity.chatRelativeId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatRoomMetadata.chatRelativeId)
            )
        }

        return Chat.RoomMetadata(
            chatRelativeId: chatRelativeId,
            name: entity.name,
            icon: entity.icon
        )
    }

    func populate(entity: CDChatRoomMetadata, from model: Chat.RoomMetadata) {
        entity.chatRelativeId = model.chatRelativeId
        entity.name = model.name
        entity.icon = model.icon
    }
}
