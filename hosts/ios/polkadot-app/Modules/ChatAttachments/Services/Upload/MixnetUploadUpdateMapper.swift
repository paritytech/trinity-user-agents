import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

final class MixnetUploadUpdateMapper {
    var entityIdentifierFieldName: String { #keyPath(CDMixnetUpload.identifier) }

    typealias DataProviderModel = MixnetUploadUpdate
    typealias CoreDataEntity = CDMixnetUpload
}

extension MixnetUploadUpdateMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        var hashes: [Data] = []

        if let chunksData = entity.uploadedChunks {
            hashes = try [Data].fromScaleEncoded(chunksData)
        }

        hashes.append(model.chunkHash)

        let encoder = ScaleEncoder()
        try hashes.encode(scaleEncoder: encoder)

        entity.uploadedChunks = encoder.encode()
        entity.uploadedSize = model.uploadedSize
    }
}
