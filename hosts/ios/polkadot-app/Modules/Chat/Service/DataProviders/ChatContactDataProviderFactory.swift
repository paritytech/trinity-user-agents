import Foundation
import Operation_iOS
import OperationExt
import CoreData
import SubstrateSdk

protocol ChatContactDataProviderMaking {
    func createAllContactsProvider() -> StreamableProvider<Chat.Contact>

    func subscribeContactsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.Contact]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject

    func subscribeChatsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.LocalModel]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject
}

final class ChatContactDataProviderFactory {
    private let repositoryFactory: ChatContactRepositoryMaking
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        repositoryFactory: ChatContactRepositoryMaking = ChatContactRepositoryFactory(),
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.repositoryFactory = repositoryFactory
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ChatContactDataProviderFactory: ChatContactDataProviderMaking {
    func createAllContactsProvider() -> StreamableProvider<Chat.Contact> {
        let repository = repositoryFactory.createRepository(forFilter: nil)
        let source = EmptyStreamableSource<Chat.Contact>()
        let mapper = ChatContactMapper()

        let observable = CoreDataContextObservable(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            predicate: { _ in true }
        )

        observable.start { [weak self] error in
            if let error {
                self?.logger.error("observable error: \(error)")
            }
        }

        return StreamableProvider(
            source: AnyStreamableSource(source),
            repository: repository,
            observable: AnyDataProviderRepositoryObservable(observable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }

    func subscribeContactsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.Contact]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDChatContact> = CDChatContact.fetchRequest()
        request.predicate = predicate
        request.sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDChatContact.username),
                ascending: true
            )
        ]

        let mapper = ChatContactMapper()
        let subscriber = CoreDataSnapshotSubscriber<Chat.Contact, CDChatContact>(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            fetchRequest: request,
            callbackQueue: queue,
            logger: logger,
            transform: { $0 },
            onUpdate: update,
            onError: failure
        )

        subscriber.start()
        return subscriber
    }

    func subscribeChatsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.LocalModel]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
        request.predicate = predicate
        request.sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDChat.lastDisplayMessage.order),
                ascending: false
            ),
            NSSortDescriptor(
                key: #keyPath(CDChat.lastDisplayMessage.timestamp),
                ascending: false
            ),
            NSSortDescriptor(
                key: #keyPath(CDChat.contact.chatRequest.timestamp),
                ascending: false
            ),
            NSSortDescriptor(
                key: #keyPath(CDChat.contact.chatRequest.status),
                ascending: true
            ),
            NSSortDescriptor(
                key: #keyPath(CDChat.identifier),
                ascending: true,
                selector: #selector(NSString.localizedCaseInsensitiveCompare)
            )
        ]

        let mapper = ChatModelMapper()
        let subscriber = CoreDataSnapshotSubscriber<Chat.LocalModel, CDChat>(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            fetchRequest: request,
            callbackQueue: queue,
            logger: logger,
            transform: { $0.sorted(by: ChatsComparator.lastMessageComparator) },
            onUpdate: update,
            onError: failure
        )

        subscriber.start()
        return subscriber
    }
}
