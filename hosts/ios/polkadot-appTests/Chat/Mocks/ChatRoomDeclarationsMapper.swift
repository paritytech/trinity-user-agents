import CoreData
import Foundation
import Operation_iOS
@testable import polkadot_app

extension Chat {
    /// The declarations of a stored room, as the read mapper gives them.
    struct StoredRoomDeclarations {
        let chatId: String
        let declarations: RoomDeclarations
    }
}

extension Chat.StoredRoomDeclarations: Identifiable {
    var identifier: String {
        chatId
    }
}

/// Reads the declarations of stored rooms. A chat without a room reads as no declarations at all.
final class ChatRoomDeclarationsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.StoredRoomDeclarations
    typealias CoreDataEntity = CDChat
}

extension ChatRoomDeclarationsMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDChat.identifier))
        }

        let declarations = Chat.RoomDeclarations(entity: entity.roomMetadata)
        return Chat.StoredRoomDeclarations(chatId: identifier, declarations: declarations)
    }

    func populate(
        entity _: CoreDataEntity,
        from _: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        throw CoreDataMapperError.unsupported
    }
}
