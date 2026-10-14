import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

final class DeclineIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.DeclineIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension DeclineIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingRequest
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from _: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard let contactId = try entity.contactId?.fromHex() else {
            throw MappingError.noExistingRequest
        }

        entity.status = Chat.RequestStatus.incoming(.declined).rawValue

        if let contact: CDChatContact = try? context.first(for: .contact(for: contactId)) {
            context.delete(contact)
        }

        if let chat: CDChat = try? context.first(for: .chatWithContact(for: contactId)) {
            context.delete(chat)
        }
    }
}

extension ChatRequest.DeclineIncoming: Identifiable {
    var identifier: String {
        requestId
    }
}
