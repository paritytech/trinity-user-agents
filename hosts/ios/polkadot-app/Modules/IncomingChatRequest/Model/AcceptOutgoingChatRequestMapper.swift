import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class AcceptOutgoingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.AcceptOutgoing
    typealias CoreDataEntity = CDChatRequest
}

extension AcceptOutgoingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingRequest
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.noExistingRequest
        }

        entity.touchParent()
        entity.contact?.acceptedAt = Date()
        entity.contact?.pendingDevicesFanOut = true
        entity.contact = nil

        guard
            let incomingRequest = model.incomingRequest,
            let acceptorDevice = model.acceptorDevice
        else {
            return
        }

        let requestEntity: CDChatRequest? = try context.first(
            for: .chatRequestById(incomingRequest.message.messageId)
        )

        let acceptIncomingMapper = AcceptIncomingChatRequestMapper()

        if let requestEntity {
            try UpdateIncomingChatRequestMapper().populate(
                entity: requestEntity,
                from: .init(remoteRequest: incomingRequest),
                using: context
            )

            requestEntity.status = Chat.RequestStatus.incoming(.accepted).rawValue

            try acceptIncomingMapper.populate(
                entity: requestEntity,
                from: .existing(
                    requestId: incomingRequest.requestId,
                    messageExchangeMode: model.messageExchangeMode,
                    acceptorDevice: acceptorDevice
                ),
                using: context
            )
        } else {
            let newEntity = try context.insertNew(CDChatRequest.self)

            try acceptIncomingMapper.populate(
                entity: newEntity,
                from: .new(
                    incomingRequest,
                    messageExchangeMode: model.messageExchangeMode,
                    acceptorDevice: acceptorDevice
                ),
                using: context
            )
        }
    }
}

extension ChatRequest.AcceptOutgoing: Identifiable {
    var identifier: String {
        requestId
    }
}
