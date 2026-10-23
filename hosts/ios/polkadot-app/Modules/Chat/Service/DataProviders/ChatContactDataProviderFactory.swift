import Foundation
import Operation_iOS
import OperationExt
import CoreData
import SubstrateSdk

protocol ChatContactDataProviderMaking {
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
