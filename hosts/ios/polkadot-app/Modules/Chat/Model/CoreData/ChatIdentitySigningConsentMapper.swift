import CoreData
import Foundation
import Operation_iOS
import SubstrateSdkExt

final class ChatIdentitySigningConsentMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.IdentitySigningConsent
    typealias CoreDataEntity = CDChatIdentitySigningConsent
}

extension ChatIdentitySigningConsentMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CoreDataEntity.identifier))
        }

        guard let grantedAt = entity.grantedAt else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CoreDataEntity.grantedAt))
        }

        return try Chat.IdentitySigningConsent(contactAccountId: identifier.fromHex(), grantedAt: grantedAt)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.grantedAt = model.grantedAt
    }
}
