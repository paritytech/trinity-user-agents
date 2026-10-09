import Foundation
import Operation_iOS
import CoreData

extension Chat {
    struct RoomFooterUpdate {
        let chatId: Chat.Id
        let footer: Chat.RoomFooter
    }
}

final class ChatRoomFooterMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.RoomFooterUpdate
    typealias CoreDataEntity = CDChat
}

extension ChatRoomFooterMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingChat
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingChat
        }

        entity.footer = model.footer.rawValue
    }
}

extension Chat.RoomFooterUpdate: Identifiable {
    var identifier: String {
        chatId.rawRepresentation
    }
}
