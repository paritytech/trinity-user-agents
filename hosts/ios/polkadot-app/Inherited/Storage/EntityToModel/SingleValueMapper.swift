import CoreData
import Foundation
import Operation_iOS

final class SingleValueMapper<T: Identifiable & Codable> {
    var entityIdentifierFieldName: String { #keyPath(CDSingleValue.identifier) }

    typealias DataProviderModel = T
    typealias CoreDataEntity = CDSingleValue
}

extension SingleValueMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        let decoder = JSONDecoder()
        return try decoder.decode(DataProviderModel.self, from: entity.payload!)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        let encoder = JSONEncoder()
        entity.identifier = model.identifier
        entity.payload = try encoder.encode(model)
    }
}
