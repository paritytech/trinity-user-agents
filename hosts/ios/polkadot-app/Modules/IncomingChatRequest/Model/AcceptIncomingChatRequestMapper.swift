import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk
import MessageExchangeKit

final class AcceptIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.AcceptIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension AcceptIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingRequest
        case requestExists
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        switch model {
        case let .existing(requestId, acceptorDevice):
            try populateExisting(entity: entity, requestId: requestId, acceptorDevice: acceptorDevice, using: context)
        case let .new(remoteModel, acceptorDevice):
            try populateNotExisting(entity: entity, remote: remoteModel, acceptorDevice: acceptorDevice, using: context)
        }
    }
}

private extension AcceptIncomingChatRequestMapper {
    func populateNotExisting(
        entity: CoreDataEntity,
        remote: ChatRequest.ValidatedRemoteModel,
        acceptorDevice: Chat.PeerDevice,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier == nil else {
            throw MappingError.requestExists
        }

        try ChatRequestMapper().populate(
            entity: entity,
            from: .init(
                requestId: remote.requestId,
                contactAccountId: remote.peerAccountId,
                timestamp: remote.message.timestamp,
                status: .incoming(.accepted),
                message: nil
            ),
            using: context
        )

        let message = try context.setupIncomingRequestMessage(model: remote)
        entity.message = message
        try context.setupIncomingRequestPeerDeviceState(model: remote)

        try context.setupAcceptMessage(
            for: remote.requestId,
            peerAccountId: remote.peerAccountId,
            acceptorDevice: acceptorDevice
        )

        try markPendingDevicesFanOut(
            for: remote.peerAccountId,
            request: entity,
            using: context
        )

        try storeRequesterDevice(
            requestId: remote.requestId,
            on: entity.contact ?? context.first(for: .contact(for: remote.peerAccountId)),
            using: context
        )
    }

    func populateExisting(
        entity: CoreDataEntity,
        requestId: String,
        acceptorDevice: Chat.PeerDevice,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.noExistingRequest
        }

        guard let peerAccountId = try entity.contactId?.fromHex() else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDChatRequest.contactId))
        }

        entity.status = Chat.RequestStatus.incoming(.accepted).rawValue
        entity.touchParent()
        entity.contact?.acceptedAt = Date()
        entity.contact?.pendingDevicesFanOut = true
        try storeRequesterDevice(requestId: requestId, on: entity.contact, using: context)
        entity.contact = nil

        try context.setupAcceptMessage(
            for: requestId,
            peerAccountId: peerAccountId,
            acceptorDevice: acceptorDevice
        )
    }

    // The requester's device is known from the request, so the session can use the device route at once.
    func storeRequesterDevice(
        requestId: String,
        on contact: CDChatContact?,
        using context: NSManagedObjectContext
    ) throws {
        guard
            let contact,
            let deviceEntity: CDChatRequestPeerDevice = try context.first(
                for: .chatRequestPeerDeviceByRequestId(requestId)
            )
        else {
            return
        }

        let device = try ChatRequestPeerDeviceMapper().transform(entity: deviceEntity).peerDevice
        try context.addPeerDevice(device, to: contact)
    }

    func markPendingDevicesFanOut(
        for peerAccountId: AccountId,
        request: CDChatRequest,
        using context: NSManagedObjectContext
    ) throws {
        let contact = try request.contact ?? context.first(for: .contact(for: peerAccountId))
        contact?.pendingDevicesFanOut = true
    }
}

extension ChatRequest.AcceptIncoming: Identifiable {
    var identifier: String {
        requestId
    }
}
