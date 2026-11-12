import Foundation
import Operation_iOS
import CoreData

extension Chat {
    struct RoomMetadataUpdate {
        let chatId: Chat.Id
        let metadata: Chat.RoomMetadata
    }
}

/// Rewrites the name and icon of a room that already exists, leaving the rest of its chat alone.
final class ChatRoomMetadataUpdateMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.RoomMetadataUpdate
    typealias CoreDataEntity = CDChat
}

extension ChatRoomMetadataUpdateMapper: CoreDataMapperProtocol {
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
        ChatRoomMetadataEntityMapper().populate(entity: metadataEntity, from: model.metadata)
        entity.roomMetadata = metadataEntity
    }
}

extension Chat.RoomMetadataUpdate: Identifiable {
    var identifier: String {
        chatId.rawRepresentation
    }
}
