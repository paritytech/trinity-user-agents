import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk
import Foundation_iOS
import SubstrateSdkExt

final class ChatRequestMapper {
    typealias DataProviderModel = Chat.Request
    typealias CoreDataEntity = CDChatRequest

    var entityIdentifierFieldName: String {
        #keyPath(CDChatRequest.identifier)
    }
}

extension ChatRequestMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let requestId = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        let timestamp = UInt64(bitPattern: entity.timestamp)

        guard let accountId = try entity.contactId?.fromHex() else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.contactId)
            )
        }

        guard let status = Chat.RequestStatus(rawValue: entity.status) else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.status)
            )
        }

        let message = try getMessage(for: entity)

        return Chat.Request(
            requestId: requestId,
            contactAccountId: accountId,
            timestamp: timestamp,
            status: status,
            message: message
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.requestId
        entity.timestamp = Int64(bitPattern: model.timestamp)
        entity.contactId = model.contactAccountId.toHex()
        entity.status = model.status.rawValue
        entity.touchParent()
    }
}

private extension ChatRequestMapper {
    func getMessage(for entity: CoreDataEntity) throws -> Chat.LocalMessage? {
        guard let message = entity.message else {
            return nil
        }

        return try ChatMessageEntityMapper().transform(entity: message)
    }
}

extension Chat.Request: Identifiable {
    var identifier: String { requestId }
}
