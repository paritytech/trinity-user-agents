import CoreData
import Foundation
import Operation_iOS

final class SSOHandledRequestMapper {
    var entityIdentifierFieldName: String { #keyPath(CDSSOHandledRequest.identifier) }

    typealias DataProviderModel = SSOHandledRequest
    typealias CoreDataEntity = CDSSOHandledRequest
}

extension SSOHandledRequestMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        return SSOHandledRequest(messageId: identifier)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
    }
}
