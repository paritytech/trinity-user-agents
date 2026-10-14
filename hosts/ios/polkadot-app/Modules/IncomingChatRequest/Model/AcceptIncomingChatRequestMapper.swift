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
        case let .existing(requestId, messageExchangeMode, acceptorDevice):
            try populateExisting(
                entity: entity,
                requestId: requestId,
                messageExchangeMode: messageExchangeMode,
                acceptorDevice: acceptorDevice,
                using: context
            )
        case let .new(remoteModel, messageExchangeMode, acceptorDevice):
            try populateNotExisting(
                entity: entity,
                remote: remoteModel,
                messageExchangeMode: messageExchangeMode,
                acceptorDevice: acceptorDevice,
                using: context
            )
        }
    }
}

private extension AcceptIncomingChatRequestMapper {
    func populateNotExisting(
        entity: CoreDataEntity,
        remote: ChatRequest.ValidatedRemoteModel,
        messageExchangeMode: MessageExchangeMode,
        acceptorDevice: Chat.PeerDevice?,
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
            messageExchangeMode: messageExchangeMode,
            acceptorDevice: acceptorDevice
        )

        try markPendingDevicesFanOut(
            for: remote.peerAccountId,
            request: entity,
            using: context
        )
    }

    func populateExisting(
        entity: CoreDataEntity,
        requestId: String,
        messageExchangeMode: MessageExchangeMode,
        acceptorDevice: Chat.PeerDevice?,
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
        entity.contact = nil

        try context.setupAcceptMessage(
            for: requestId,
            peerAccountId: peerAccountId,
            messageExchangeMode: messageExchangeMode,
            acceptorDevice: acceptorDevice
        )
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
