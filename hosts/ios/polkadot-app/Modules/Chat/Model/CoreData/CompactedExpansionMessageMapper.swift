import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

final class CompactedExpansionMessageMapper {
    struct Model: Identifiable {
        let compactedMessage: Chat.LocalMessage
        let expandedMessages: [Chat.RemoteMessage]

        var identifier: String { compactedMessage.messageId }
    }

    typealias DataProviderModel = Model
    typealias CoreDataEntity = CDChatMessage

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }
}

extension CompactedExpansionMessageMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.messageId != nil else {
            throw CoreDataMapperError.unexpected("Message must exist")
        }

        guard case .compactedMessages = model.compactedMessage.content else {
            throw CoreDataMapperError.unexpected("Message must be compacted")
        }

        entity.contentExpanded = true

        let expandedMessageMapper = ChatMessageEntityMapper()

        for remoteMessage in model.expandedMessages {
            if let localMessage = Chat.LocalMessage(
                remote: remoteMessage,
                creationSource: .localDevice,
                status: .incoming(.new),
                chatId: model.compactedMessage.chatId,
                origin: model.compactedMessage.origin
            ) {
                let expandedEntity: CDChatMessage = try context.first(
                    for: .chatMessage(with: remoteMessage.messageId)
                ) ?? context.insertNew(CDChatMessage.self)

                try expandedMessageMapper.populate(
                    entity: expandedEntity,
                    from: localMessage,
                    inheritingOrder: entity.order,
                    using: context
                )
            }
        }
    }
}
