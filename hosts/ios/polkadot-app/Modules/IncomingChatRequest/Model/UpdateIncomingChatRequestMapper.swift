import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class UpdateIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.UpdateIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension UpdateIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case notExistingRequest
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
            throw MappingError.notExistingRequest
        }

        entity.timestamp = Int64(bitPattern: model.remoteRequest.message.timestamp)
        try context.setupIncomingRequestPeerDeviceState(model: model.remoteRequest)
        entity.touchParent()

        let versionedContent = model.remoteRequest.message.content

        if let contact = entity.contact {
            let pushToken = versionedContent.ensureV1().pushToken
            contact.pushToken = pushToken?.token
            contact.pushPlatform = pushToken?.pushType.platform.rawValue

            // Do NOT store peer devices yet — keep devices=[] so the session stays
            // identity-level during the handshake. Devices will be stored after
            // multiChatAccepted is exchanged.
        }

        if let message = entity.message {
            let localMessage = Chat.LocalMessage(
                chatRequest: model.remoteRequest.message,
                creationSource: .localDevice,
                status: .incoming(.new),
                contactId: model.remoteRequest.peerAccountId
            )
            try ChatMessageEntityMapper().populate(
                entity: message,
                from: localMessage,
                using: context
            )
        }
    }
}

extension ChatRequest.UpdateIncoming: Identifiable {
    var identifier: String {
        remoteRequest.message.messageId
    }
}
