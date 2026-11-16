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
        context.deleteChatContactAndChat(for: contactId)
    }
}

extension ChatRequest.DeclineIncoming: Identifiable {
    var identifier: String {
        requestId
    }
}
