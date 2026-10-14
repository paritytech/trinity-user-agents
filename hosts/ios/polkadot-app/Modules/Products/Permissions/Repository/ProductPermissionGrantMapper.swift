import CoreData
import Operation_iOS
import Products

final class ProductPermissionGrantMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ProductPermissionGrant
    typealias CoreDataEntity = CDProductPermissionGrant
}

extension ProductPermissionGrantMapper: CoreDataMapperProtocol {
    func transform(entity: CDProductPermissionGrant) throws -> ProductPermissionGrant {
        guard let productId = entity.productId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductPermissionGrant.productId)
            )
        }

        guard let typeName = entity.permissionType else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductPermissionGrant.permissionType)
            )
        }

        guard let key = entity.permissionKey else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProductPermissionGrant.permissionKey)
            )
        }

        guard let permission = ProductPermission.from(typeName: typeName, key: key) else {
            throw ProductPermissionGrantMapperError.unknownPermission(typeName: typeName, key: key)
        }

        return ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: entity.granted,
            grantedAt: entity.grantedAt
        )
    }

    func populate(
        entity: CDProductPermissionGrant,
        from model: ProductPermissionGrant,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.productId = model.productId
        entity.permissionType = model.permission.typeName
        entity.permissionKey = model.permission.key
        entity.granted = model.granted
        entity.grantedAt = model.grantedAt
    }
}

enum ProductPermissionGrantMapperError: Error {
    case unknownPermission(typeName: String, key: String)
}
