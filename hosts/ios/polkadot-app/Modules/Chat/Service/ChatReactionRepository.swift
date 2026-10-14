import Foundation
import Operation_iOS
import CoreData

protocol ChatReactionRepositoryProtocol {
    func saveReaction(_ reaction: Chat.MessageReaction) -> CompoundOperationWrapper<Void>

    func updateReaction(
        _ reaction: Chat.MessageReaction,
        removing reactionsToRemove: [Chat.MessageReaction]
    ) -> CompoundOperationWrapper<Void>

    func removeReaction(
        messageId: String,
        emoji: String,
        origin: Chat.LocalMessage.Origin
    ) -> CompoundOperationWrapper<Void>

    func removeReactions(
        _ reactions: [Chat.MessageReaction]
    ) -> CompoundOperationWrapper<Void>

    func fetchReactions(for messageId: String) -> CompoundOperationWrapper<[Chat.MessageReaction]>

    func fetchReactions(for messageIds: [String]) -> CompoundOperationWrapper<[String: [Chat.MessageReaction]]>
}

final class ChatReactionRepository {
    private let repository: AnyDataProviderRepository<Chat.MessageReaction>
    private let operationQueue: OperationQueue

    init(
        repository: AnyDataProviderRepository<Chat.MessageReaction>,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue
    ) {
        self.repository = repository
        self.operationQueue = operationQueue
    }
}

extension ChatReactionRepository: ChatReactionRepositoryProtocol {
    func saveReaction(
        _ reaction: Chat.MessageReaction
    ) -> CompoundOperationWrapper<Void> {
        let saveOperation = repository.saveOperation({
            [reaction]
        }, { [] })

        return CompoundOperationWrapper(targetOperation: saveOperation)
    }

    func updateReaction(
        _ reaction: Chat.MessageReaction,
        removing reactionsToRemove: [Chat.MessageReaction]
    ) -> CompoundOperationWrapper<Void> {
        let idsToRemove = reactionsToRemove.map {
            "\($0.messageId)_\($0.emoji)_\($0.origin.rawType)"
        }

        let saveOperation = repository.saveOperation({
            [reaction]
        }, {
            idsToRemove
        })

        return CompoundOperationWrapper(targetOperation: saveOperation)
    }

    func removeReaction(
        messageId: String,
        emoji: String,
        origin: Chat.LocalMessage.Origin
    ) -> CompoundOperationWrapper<Void> {
        let identifier = "\(messageId)_\(emoji)_\(origin.rawType)"

        let removeOperation = repository.saveOperation(
            { [] },
            { [identifier] }
        )

        return CompoundOperationWrapper(targetOperation: removeOperation)
    }

    func removeReactions(
        _ reactions: [Chat.MessageReaction]
    ) -> CompoundOperationWrapper<Void> {
        let idsToRemove = reactions.map {
            "\($0.messageId)_\($0.emoji)_\($0.origin.rawType)"
        }

        let saveOperation = repository.saveOperation({ [] }, {
            idsToRemove
        })

        return CompoundOperationWrapper(targetOperation: saveOperation)
    }

    func fetchReactions(
        for messageId: String
    ) -> CompoundOperationWrapper<[Chat.MessageReaction]> {
        let fetchOperation = repository.fetchAllOperation(
            with: RepositoryFetchOptions()
        )

        let filterOperation = ClosureOperation<[Chat.MessageReaction]> {
            let allReactions = try fetchOperation.extractNoCancellableResultData()
            return allReactions.filter { $0.messageId == messageId }
        }

        filterOperation.addDependency(fetchOperation)

        return CompoundOperationWrapper(
            targetOperation: filterOperation,
            dependencies: [fetchOperation]
        )
    }

    func fetchReactions(
        for messageIds: [String]
    ) -> CompoundOperationWrapper<[String: [Chat.MessageReaction]]> {
        let fetchOperation = repository.fetchAllOperation(
            with: RepositoryFetchOptions()
        )

        let groupOperation = ClosureOperation<[String: [Chat.MessageReaction]]> {
            let allReactions = try fetchOperation.extractNoCancellableResultData()
            let messageIdSet = Set(messageIds)

            var grouped: [String: [Chat.MessageReaction]] = [:]
            for reaction in allReactions where messageIdSet.contains(reaction.messageId) {
                grouped[reaction.messageId, default: []].append(reaction)
            }

            return grouped
        }

        groupOperation.addDependency(fetchOperation)

        return CompoundOperationWrapper(
            targetOperation: groupOperation,
            dependencies: [fetchOperation]
        )
    }
}
