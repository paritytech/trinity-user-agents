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

        // One save: a crash can never leave an accepted chat without the device its session needs.
        if let peerDevice = model.peerDevice {
            if let contact = try entity.contact ?? contact(for: entity, using: context) {
                try context.addPeerDevice(peerDevice, to: contact)
            }

            entity.message?.status = Chat.LocalMessage.Status.outgoing(.delivered).rawValue
            entity.message?.touchParent()
        }

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
                from: .existing(requestId: incomingRequest.requestId, acceptorDevice: acceptorDevice),
                using: context
            )
        } else {
            let newEntity = try context.insertNew(CDChatRequest.self)

            try acceptIncomingMapper.populate(
                entity: newEntity,
                from: .new(incomingRequest, acceptorDevice: acceptorDevice),
                using: context
            )
        }
    }
}

private extension AcceptOutgoingChatRequestMapper {
    func contact(for request: CDChatRequest, using context: NSManagedObjectContext) throws -> CDChatContact? {
        guard let contactAccountId = try request.contactId?.fromHex() else { return nil }

        return try context.first(for: .contact(for: contactAccountId))
    }
}

extension ChatRequest.AcceptOutgoing: Identifiable {
    var identifier: String {
        requestId
    }
}
