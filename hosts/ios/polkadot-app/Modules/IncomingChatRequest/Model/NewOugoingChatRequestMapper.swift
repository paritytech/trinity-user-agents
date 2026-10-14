import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class NewOutgoingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.NewOutgoing
    typealias CoreDataEntity = CDChatRequest
}

extension NewOutgoingChatRequestMapper: CoreDataMapperProtocol {
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

        entity.identifier = model.message.messageId
        entity.timestamp = Int64(bitPattern: model.message.timestamp)
        entity.contactId = model.remoteContact.accountId.toHex()
        entity.status = Chat.RequestStatus.outgoing.rawValue

        let contact = try setupContact(model: model, context: context)

        entity.contact = contact

        let localContact = Chat.Contact(remoteContact: model.remoteContact, ownKeyId: model.ownKeyId)
        try setupChat(localContact: localContact, context: context)

        let message = try context.setupOutgoingRequestMessage(
            model.message,
            peerAccountId: model.remoteContact.accountId
        )

        entity.message = message
    }
}

private extension NewOutgoingChatRequestMapper {
    func setupContact(
        model: ChatRequest.NewOutgoing,
        context: NSManagedObjectContext
    ) throws -> CDChatContact {
        let optContact: CDChatContact? = try context.first(for: .contact(for: model.remoteContact.accountId))
        guard optContact == nil else {
            throw MappingError.existingContact
        }

        var contactModel = Chat.Contact(remoteContact: model.remoteContact, ownKeyId: model.ownKeyId)
        contactModel.pushId = model.pushId

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

extension ChatRequest.NewOutgoing: Identifiable {
    var identifier: String {
        message.messageId
    }
}
