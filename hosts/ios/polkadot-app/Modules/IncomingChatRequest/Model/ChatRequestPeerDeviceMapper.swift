import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

extension ChatRequest {
    struct PeerDeviceForRequest: Equatable {
        let requestId: String
        let peerDevice: Chat.PeerDevice
    }
}

final class ChatRequestPeerDeviceMapper {
    typealias DataProviderModel = ChatRequest.PeerDeviceForRequest
    typealias CoreDataEntity = CDChatRequestPeerDevice

    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.requestId)
    }
}

extension ChatRequestPeerDeviceMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let requestId = entity.requestId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.requestId)
            )
        }

        guard let statementAccountId = entity.statementAccountId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.statementAccountId)
            )
        }

        guard let encryptionPublicKey = entity.encryptionPublicKey else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.encryptionPublicKey)
            )
        }

        return ChatRequest.PeerDeviceForRequest(
            requestId: requestId,
            peerDevice: Chat.PeerDevice(
                statementAccountId: statementAccountId,
                encryptionPublicKey: encryptionPublicKey
            )
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.requestId = model.requestId
        entity.statementAccountId = model.peerDevice.statementAccountId
        entity.encryptionPublicKey = model.peerDevice.encryptionPublicKey
    }
}

extension ChatRequest.PeerDeviceForRequest: Identifiable {
    var identifier: String {
        requestId
    }
}

extension NSManagedObjectContext {
    func setupIncomingRequestPeerDeviceState(model: ChatRequest.ValidatedRemoteModel) throws {
        guard let peerDevice = model.peerDevice else {
            return
        }

        let predicate = NSPredicate.chatRequestPeerDeviceByRequestId(model.requestId)
        let entity: CDChatRequestPeerDevice = try first(for: predicate) ?? CDChatRequestPeerDevice(context: self)
        let requestPeerDevice = ChatRequest.PeerDeviceForRequest(
            requestId: model.requestId,
            peerDevice: peerDevice
        )

        try ChatRequestPeerDeviceMapper().populate(
            entity: entity,
            from: requestPeerDevice,
            using: self
        )

        // TODO: - remove when not needed
        // Workaround - save the device added message with the initial
        // device so it will be synced with the desktop app
        try setupDeviceAddedMessage(
            peerDevice: peerDevice,
            peerAccountId: model.peerAccountId
        )
    }

    private func setupDeviceAddedMessage(
        peerDevice: Chat.PeerDevice,
        peerAccountId: AccountId
    ) throws {
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .localMessages(from: .person(peerAccountId)),
            .messageByContentType(.deviceAdded)
        ])

        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = predicate

        let alreadyExists = try fetch(request).contains { entity in
            guard case let .deviceAdded(content) = try ChatMessageEntityMapper.getContent(from: entity) else {
                return false
            }
            return content.statementAccountId == peerDevice.statementAccountId &&
                content.encryptionPublicKey == peerDevice.encryptionPublicKey
        }

        guard !alreadyExists else {
            return
        }

        try insertDeviceAddedMessage(
            peerDevice: peerDevice,
            peerAccountId: peerAccountId
        )
    }

    private func insertDeviceAddedMessage(
        peerDevice: Chat.PeerDevice,
        peerAccountId: AccountId
    ) throws {
        let content = Chat.LocalMessage.Content.deviceAdded(
            .init(
                statementAccountId: peerDevice.statementAccountId,
                encryptionPublicKey: peerDevice.encryptionPublicKey
            )
        )

        let message = Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: .person(peerAccountId),
            origin: .contact(peerAccountId),
            creationSource: .localDevice,
            status: .incoming(.new),
            timestamp: Date().toChatTimestamp(),
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let entity = CDChatMessage(context: self)

        try ChatMessageEntityMapper().populate(
            entity: entity,
            from: message,
            using: self
        )
    }
}

extension NSPredicate {
    static func chatRequestPeerDeviceByRequestId(_ requestId: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatRequestPeerDevice.requestId), requestId)
    }
}
