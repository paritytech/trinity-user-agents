import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class ReplaceIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.ReplaceIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension ReplaceIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case existingRequest
        case noContact
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier == nil else {
            throw MappingError.existingRequest
        }

        let contact: CDChatContact? = try context.first(for: .contact(for: model.remoteRequest.peerAccountId))

        guard let contact else {
            throw MappingError.noContact
        }

        try ChatRequestMapper().populate(
            entity: entity,
            from: .init(
                requestId: model.remoteRequest.requestId,
                contactAccountId: model.remoteRequest.peerAccountId,
                timestamp: model.remoteRequest.message.timestamp,
                status: .incoming(.new),
                message: nil
            ),
            using: context
        )

        let versionedContent = model.remoteRequest.message.content
        let pushToken = versionedContent.ensureV1().pushToken
        contact.pushToken = pushToken?.token
        contact.pushPlatform = pushToken?.pushType.platform.rawValue

        // Do NOT store peer devices yet — keep devices=[] so the session stays
        // identity-level during the handshake. Devices will be stored after
        // multiChatAccepted is exchanged.

        contact.chatRequest = entity

        let message = try context.setupIncomingRequestMessage(model: model.remoteRequest)
        entity.message = message
        try context.setupIncomingRequestPeerDeviceState(model: model.remoteRequest)
        entity.touchParent()
    }
}

extension ChatRequest.ReplaceIncoming: Identifiable {
    var identifier: String {
        remoteRequest.message.messageId
    }
}
