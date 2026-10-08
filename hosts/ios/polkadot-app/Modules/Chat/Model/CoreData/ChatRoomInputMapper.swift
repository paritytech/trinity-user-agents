import Foundation
import Operation_iOS
import CoreData

extension Chat {
    struct RoomInputVisibility {
        let chatId: Chat.Id
        let roomId: String
        let hidesTextInput: Bool
    }
}

final class ChatRoomInputMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.RoomInputVisibility
    typealias CoreDataEntity = CDChat
}

extension ChatRoomInputMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingChat
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingChat
        }

        let metadataEntity = try entity.roomMetadata ?? context.insertNew(CDChatRoomMetadata.self)
        metadataEntity.chatRelativeId = metadataEntity.chatRelativeId ?? model.roomId
        metadataEntity.hidesTextInput = model.hidesTextInput
        entity.roomMetadata = metadataEntity
    }
}

extension Chat.RoomInputVisibility: Identifiable {
    var identifier: String {
        chatId.rawRepresentation
    }
}
