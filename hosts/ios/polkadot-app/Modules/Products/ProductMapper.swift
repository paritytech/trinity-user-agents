import CoreData
import Operation_iOS
import Products

final class ProductMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Product
    typealias CoreDataEntity = CDProduct
}

extension ProductMapper: CoreDataMapperProtocol {
    func transform(entity: CDProduct) throws -> Product {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProduct.identifier)
            )
        }

        guard let name = entity.name else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDProduct.name)
            )
        }

        return Product(id: identifier, name: name)
    }

    func populate(
        entity: CDProduct,
        from model: Product,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.name = model.name
    }
}
