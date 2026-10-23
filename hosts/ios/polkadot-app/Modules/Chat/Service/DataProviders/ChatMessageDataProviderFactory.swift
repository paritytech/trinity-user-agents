import Foundation
import Operation_iOS
import OperationExt
import CoreData
import SubstrateSdk

protocol ChatMessageDataProviderMaking {
    func subscribeMessagesSnapshot(
        with predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.LocalMessage]) -> Void
    ) -> AnyObject
}

final class ChatMessageDataProviderFactory {
    private let repositoryFactory: ChatMessageRepositoryMaking
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        repositoryFactory: ChatMessageRepositoryMaking = ChatMessageRepositoryFactory(),
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.repositoryFactory = repositoryFactory
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ChatMessageDataProviderFactory: ChatMessageDataProviderMaking {
    func subscribeMessagesSnapshot(
        with predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.LocalMessage]) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = predicate
        // Matches `byChatOrderTimestampIndex (chat, order, timestamp)` so the feed reads rows pre-sorted.
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: true),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: true)
        ]

        let mapper = ChatMessageEntityMapper()
        let subscriber = CoreDataSnapshotSubscriber<Chat.LocalMessage, CDChatMessage>(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            fetchRequest: request,
            callbackQueue: queue,
            logger: logger,
            onUpdate: update
        )

        subscriber.start()
        return subscriber
    }
}
