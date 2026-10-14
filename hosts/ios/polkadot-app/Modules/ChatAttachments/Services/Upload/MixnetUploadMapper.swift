import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

final class MixnetUploadMapper {
    var entityIdentifierFieldName: String { #keyPath(CDMixnetUpload.identifier) }

    typealias DataProviderModel = MixnetUpload
    typealias CoreDataEntity = CDMixnetUpload
}

extension MixnetUploadMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let attachmentId = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        let uploadedHashes: [Data]? = try entity.uploadedChunks.map { try [Data].fromScaleEncoded($0) }

        return MixnetUpload(
            attachmentId: attachmentId,
            ticket: entity.ticket,
            node: entity.node,
            uploadedHashes: uploadedHashes,
            uploadedSize: entity.uploadedSize
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.ticket = model.ticket
        entity.node = model.node

        if let hashes = model.uploadedHashes {
            entity.uploadedChunks = try hashes.scaleEncoded()
        } else {
            entity.uploadedChunks = nil
        }

        entity.uploadedSize = model.uploadedSize
    }
}
