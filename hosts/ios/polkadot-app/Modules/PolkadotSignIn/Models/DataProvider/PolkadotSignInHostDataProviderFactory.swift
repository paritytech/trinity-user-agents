import Foundation
import Foundation_iOS
import Operation_iOS
import OperationExt
import AsyncExtensions
import CoreData

protocol PolkadotSignInHostDataProviderMaking {
    func subscribeHostsSnapshot(
        deliverOn queue: DispatchQueue,
        update: @escaping ([PolkadotSignInHost]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject

    func subscribeHosts() -> AnyAsyncSequence<[PolkadotSignInHost]>
}

final class PolkadotSignInHostDataProviderFactory {
    private let repositoryFactory: PolkadotSignInHostRepositoryMaking
    private let logger: LoggerProtocol

    private var asyncProviders = InMemoryCache<String, AnyObject>()

    init(
        repositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.repositoryFactory = repositoryFactory
        self.logger = logger
    }
}

extension PolkadotSignInHostDataProviderFactory: PolkadotSignInHostDataProviderMaking {
    func subscribeHostsSnapshot(
        deliverOn queue: DispatchQueue,
        update: @escaping ([PolkadotSignInHost]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDPolkadotSignInHost> = CDPolkadotSignInHost.fetchRequest()
        request.sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDPolkadotSignInHost.name),
                ascending: true
            )
        ]
        let mapper = PolkadotSignInHostMapper()
        let subscriber = CoreDataSnapshotSubscriber<PolkadotSignInHost, CDPolkadotSignInHost>(
            service: repositoryFactory.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            fetchRequest: request,
            callbackQueue: queue,
            logger: logger,
            onUpdate: update,
            onError: failure
        )
        subscriber.start()
        return subscriber
    }

    func subscribeHosts() -> AnyAsyncSequence<[PolkadotSignInHost]> {
        let syncQueue = DispatchQueue(label: "io.sign.in.host.provider.async.updates")

        return AsyncStream { [weak self] continuation in
            guard let self else {
                return
            }

            let uuid = UUID().uuidString
            let cache = asyncProviders
            let provider = subscribeHostsSnapshot(
                deliverOn: syncQueue,
                update: { continuation.yield($0) },
                failure: { [logger] in logger.error("\($0)") }
            )

            cache.store(value: provider, for: uuid)

            continuation.onTermination = { @Sendable _ in
                cache.clear(for: uuid)
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
