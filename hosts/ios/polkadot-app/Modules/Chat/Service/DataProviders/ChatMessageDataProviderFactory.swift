import Foundation
import Operation_iOS
import OperationExt
import CoreData
import SubstrateSdk

protocol ChatMessageDataProviderMaking {
    func createNewRemoteMessagesLifecycleProvider() -> StreamableProvider<Chat.LocalMessage>

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
    func createNewRemoteMessagesLifecycleProvider() -> StreamableProvider<Chat.LocalMessage> {
        let filter = NSPredicate.newOutgoingRemoteMessages()
        let repository = repositoryFactory.createRepository(forFilter: filter)
        let source = EmptyStreamableSource<Chat.LocalMessage>()
        let mapper = ChatMessageEntityMapper()

        let observable = CoreDataContextObservable(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            predicate: { entity in
                entity.chat?.chatType == Int16(Chat.Id.ChatType.person.rawValue)
            } // do not filter (to pass update when status changed from target .new to other)
        )

        observable.start { [weak self] error in
            if let error {
                self?.logger.error("Did receive error: \(error)")
            }
        }

        return StreamableProvider(
            source: AnyStreamableSource(source),
            repository: repository,
            observable: AnyDataProviderRepositoryObservable(observable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }

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
