import CoreData
import Operation_iOS

final class ProductOperationMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ProductOperationRecord
    typealias CoreDataEntity = CDProductOperation
}

extension ProductOperationMapper: CoreDataMapperProtocol {
    func transform(entity: CDProductOperation) throws -> ProductOperationRecord {
        guard let productId = entity.productId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductOperation.productId)
            )
        }

        guard let startedAt = entity.startedAt else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductOperation.startedAt)
            )
        }

        guard let id = UInt32(exactly: entity.operationId) else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductOperation.operationId)
            )
        }

        return ProductOperationRecord(
            productId: productId,
            id: id,
            label: entity.label,
            startedAt: startedAt
        )
    }

    func populate(
        entity: CDProductOperation,
        from model: ProductOperationRecord,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.productId = model.productId
        entity.operationId = Int64(model.id)
        entity.label = model.label
        entity.startedAt = model.startedAt
    }
}
