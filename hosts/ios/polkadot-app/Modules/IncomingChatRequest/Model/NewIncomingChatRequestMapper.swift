import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class NewIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.NewIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension NewIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case existingRequest
        case existingContact
        case existingChat
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

        entity.identifier = model.identifier
        entity.timestamp = Int64(bitPattern: model.remoteRequest.message.timestamp)
        entity.contactId = model.remoteContact.accountId.toHex()
        entity.status = Chat.RequestStatus.incoming(.new).rawValue

        let contact = try setupContact(
            model: model,
            context: context
        )

        entity.contact = contact

        let localContact = Chat.Contact(remoteContact: model.remoteContact, ownKeyId: model.ownKeyId)
        try setupChat(localContact: localContact, context: context)

        let message = try context.setupIncomingRequestMessage(model: model.remoteRequest)
        entity.message = message

        try context.setupIncomingRequestPeerDeviceState(model: model.remoteRequest)
    }
}

private extension NewIncomingChatRequestMapper {
    func setupContact(
        model: ChatRequest.NewIncoming,
        context: NSManagedObjectContext
    ) throws -> CDChatContact {
        let optContact: CDChatContact? = try context.first(for: .contact(for: model.remoteRequest.peerAccountId))
        guard optContact == nil else {
            throw MappingError.existingContact
        }

        var contactModel = Chat.Contact(remoteContact: model.remoteContact, ownKeyId: model.ownKeyId)
        let versionedContent = model.remoteRequest.message.content
        let pushToken = versionedContent.ensureV1().pushToken
        contactModel.pushToken = pushToken?.token
        contactModel.peerPlatform = pushToken?.pushType.platform
        contactModel.pushId = model.pushId

        // Do NOT store peer devices yet — keep devices=[] so the session stays
        // identity-level during the handshake. Devices will be stored after
        // DeviceChatAccepted is exchanged.

        let contact = try context.insertNew(CDChatContact.self)
        try ChatContactMapper().populate(
            entity: contact,
            from: contactModel,
            using: context
        )

        return contact
    }

    func setupChat(
        localContact: Chat.Contact,
        context: NSManagedObjectContext
    ) throws {
        let optChat: CDChat? = try context.first(for: .chatWithContact(for: localContact.accountId))

        guard optChat == nil else {
            throw MappingError.existingChat
        }

        let chat = try context.insertNew(CDChat.self)

        try ChatModelMapper().populate(
            entity: chat,
            from: Chat.LocalModel.newChatWithContact(localContact),
            using: context
        )
    }
}

extension ChatRequest.NewIncoming: Identifiable {
    var identifier: String {
        remoteRequest.message.messageId
    }
}
