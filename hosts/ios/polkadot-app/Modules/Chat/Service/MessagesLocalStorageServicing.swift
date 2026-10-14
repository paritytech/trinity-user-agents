import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk
import OperationExt

protocol MessagesLocalStorageServicing {
    func message(
        with messageIdClosure: @escaping () throws -> String
    ) -> BaseOperation<Chat.LocalMessage?>

    func insertOrUpdate(
        _ messagesClosure: @escaping () throws -> [Chat.LocalMessage]
    ) -> BaseOperation<Void>

    func insertOrUpdate(_ messages: [Chat.LocalMessage]) -> BaseOperation<Void>

    func markAsSent(_ messageIds: [String]) -> CompoundOperationWrapper<Void>
    func markAsDelivered(_ messageIds: [String]) -> CompoundOperationWrapper<Void>
    func markAsSeen(_ messageIds: [String]) -> CompoundOperationWrapper<Void>

    func markSentAsNewIfMissingIn(
        messageIds: Set<String>,
        contactId: AccountId
    ) -> CompoundOperationWrapper<Void>

    func fetchExpandedMessages(
        for compactionIds: Set<String>
    ) -> CompoundOperationWrapper<[Chat.LocalMessage]>

    func commitCompaction(
        compactedRemoteMessage: Chat.RemoteMessage,
        originalMessageIds: [String]
    ) -> BaseOperation<Void>
}

final class MessagesLocalStorageService: MessagesLocalStorageServicing {
    private let repositoryFactory: ChatMessageRepositoryMaking
    private let statusUpdateRepositoryFactory: ChatMessageStatusUpdateRepositoryMaking
    private let compactionCommitRepository: AnyDataProviderRepository<CompactionCommitMapper.Model>
    private let logger: LoggerProtocol
    private let operationQueue: OperationQueue

    init(
        repositoryFactory: ChatMessageRepositoryMaking = ChatMessageRepositoryFactory(),
        statusUpdateRepositoryFactory: ChatMessageStatusUpdateRepositoryMaking =
            ChatMessageStatusUpdateRepositoryFactory(),
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.repositoryFactory = repositoryFactory
        self.statusUpdateRepositoryFactory = statusUpdateRepositoryFactory
        self.operationQueue = operationQueue
        self.logger = logger

        compactionCommitRepository = storageFacade.createRepository(
            mapper: CompactionCommitMapper().eraseType()
        )
        .eraseType()
    }

    func message(
        with messageIdClosure: @escaping () throws -> String
    ) -> BaseOperation<Chat.LocalMessage?> {
        let repository = repositoryFactory.createRepository(forFilter: nil)
        return repository.fetchOperation(by: messageIdClosure, options: .init())
    }

    func insertOrUpdate(
        _ messagesClosure: @escaping () throws -> [Chat.LocalMessage]
    ) -> BaseOperation<Void> {
        let repository = repositoryFactory.createRepository(forFilter: nil)
        return repository.saveOperation(messagesClosure) { [] }
    }

    func insertOrUpdate(_ messages: [Chat.LocalMessage]) -> BaseOperation<Void> {
        let repository = repositoryFactory.createRepository(forFilter: nil)
        return repository.saveOperation({ messages }, { [] })
    }

    func markAsSent(_ messageIds: [String]) -> CompoundOperationWrapper<Void> {
        updateStatusWrapper(for: messageIds, to: .outgoing(.sent))
    }

    func markAsDelivered(_ messageIds: [String]) -> CompoundOperationWrapper<Void> {
        updateStatusWrapper(for: messageIds, to: .outgoing(.delivered))
    }

    func markAsSeen(_ messageIds: [String]) -> CompoundOperationWrapper<Void> {
        updateStatusWrapper(for: messageIds, to: .incoming(.seen))
    }

    func fetchExpandedMessages(
        for compactionIds: Set<String>
    ) -> CompoundOperationWrapper<[Chat.LocalMessage]> {
        expandedMessagesWrapper(for: compactionIds, accumulatedMessages: [])
    }

    func commitCompaction(
        compactedRemoteMessage: Chat.RemoteMessage,
        originalMessageIds: [String]
    ) -> BaseOperation<Void> {
        let commitModel = CompactionCommitMapper.Model(
            compactedRemoteMessage: compactedRemoteMessage,
            originalMessageIds: originalMessageIds
        )

        return compactionCommitRepository.saveOperation({ [commitModel] }, { [] })
    }

    func markSentAsNewIfMissingIn(
        messageIds: Set<String>,
        contactId: AccountId
    ) -> CompoundOperationWrapper<Void> {
        let filter = NSPredicate.sentLocalDeviceMessages(to: .person(contactId))

        let repository = statusUpdateRepositoryFactory.createRepository(forFilter: filter)

        let fetchAll = repository.fetchAllOperation(with: .init())

        let filteredUpdates = ClosureOperation<[Chat.ChatMessageStatusUpdate]> {
            let messagesToUpdate = try fetchAll.extractNoCancellableResultData()

            return messagesToUpdate.compactMap { messageToUpdate in
                guard !messageIds.contains(messageToUpdate.messageId) else {
                    return nil
                }

                return messageToUpdate.replacingStatus(.outgoing(.new))
            }
        }

        filteredUpdates.addDependency(fetchAll)

        let saveOperation = repository.saveOperation({
            try filteredUpdates.extractNoCancellableResultData()
        }, {
            []
        })

        saveOperation.addDependency(filteredUpdates)

        return CompoundOperationWrapper(
            targetOperation: saveOperation,
            dependencies: [fetchAll, filteredUpdates]
        )
    }
}

// MARK: - Helpers

private extension MessagesLocalStorageService {
    func updateStatusWrapper(
        for messageIds: [String],
        to newStatus: Chat.LocalMessage.Status
    ) -> CompoundOperationWrapper<Void> {
        guard !messageIds.isEmpty else {
            return .createWithResult(())
        }

        let repository = statusUpdateRepositoryFactory.createRepository(
            forFilter: .messages(withIds: messageIds)
        )

        let fetchExisting = repository.fetchAllOperation(with: .init())

        let saveOperation = repository.saveOperation({
            try fetchExisting.extractNoCancellableResultData().map {
                $0.replacingStatus(newStatus)
            }
        }, { [] })

        saveOperation.addDependency(fetchExisting)

        return CompoundOperationWrapper(
            targetOperation: saveOperation,
            dependencies: [fetchExisting]
        )
    }

    func fetchByCompactionIds(_ compactionIds: Set<String>) -> BaseOperation<[Chat.LocalMessage]> {
        let filter = NSPredicate.messagesByCompactionIds(compactionIds)
        let repository = repositoryFactory.createRepository(forFilter: filter)
        return repository.fetchAllOperation(with: RepositoryFetchOptions())
    }

    func expandedMessagesWrapper(
        for compactionIds: Set<String>,
        accumulatedMessages: [Chat.LocalMessage]
    ) -> CompoundOperationWrapper<[Chat.LocalMessage]> {
        guard !compactionIds.isEmpty else {
            return .createWithResult(accumulatedMessages)
        }

        let fetchOperation = fetchByCompactionIds(compactionIds)

        let wrapper = OperationCombiningService<[Chat.LocalMessage]>.compoundNonOptionalWrapper(
            operationQueue: operationQueue
        ) {
            let allMessageIds = Set(accumulatedMessages.map(\.messageId))
            let directMessages = try fetchOperation
                .extractNoCancellableResultData()
                .filter { !allMessageIds.contains($0.messageId) }

            let nestedCompactionIds = Set(
                directMessages
                    .filter { $0.content.contentType == .compactedMessages }
                    .map(\.messageId)
            )

            let newMessages = accumulatedMessages + directMessages

            guard !nestedCompactionIds.isEmpty else {
                return .createWithResult(newMessages)
            }

            return self.expandedMessagesWrapper(for: nestedCompactionIds, accumulatedMessages: newMessages)
        }

        wrapper.addDependency(operations: [fetchOperation])

        return wrapper.insertingHead(operations: [fetchOperation])
    }
}
