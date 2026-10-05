import Foundation
import Foundation_iOS
import Operation_iOS
import OperationExt
import CoreData

protocol LocalDeviceDataProviderMaking {
    func subscribeDevices() -> AsyncStream<[Chat.LocalDevice]>
}

final class LocalDeviceDataProviderFactory {
    private let repositoryFactory: LocalDeviceRepositoryMaking
    private let logger: LoggerProtocol

    private var asyncProviders = InMemoryCache<String, AnyObject>()

    init(
        repositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.repositoryFactory = repositoryFactory
        self.logger = logger
    }
}

extension LocalDeviceDataProviderFactory: LocalDeviceDataProviderMaking {
    func subscribeDevices() -> AsyncStream<[Chat.LocalDevice]> {
        if SettingsManager.shared.isTrUAPIRuntimeEnabled {
            return AsyncStream { continuation in
                let task = Task { [logger] in
                    for await _ in truapiRecordChanges(.truapiPairedHostsChanged) {
                        do {
                            guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
                                throw ProductBotFactoryError.dependenciesUnavailable
                            }
                            let peers = try await provider.activeRuntimeForRecords()?.pairedHosts() ?? []
                            continuation.yield(peers.map { peer in
                                Chat.LocalDevice(
                                    statementAccountId: peer.peerStatement,
                                    encryptionPublicKey: peer.peerEncryption,
                                    hostName: peer.metadata["hostName"] ?? "",
                                    createdAt: Date(timeIntervalSince1970: Double(peer.addedAt) / 1000),
                                    hostVersion: peer.metadata["hostVersion"],
                                    osType: peer.metadata["platformType"],
                                    osVersion: peer.metadata["platformVersion"]
                                )
                            })
                        } catch { logger.error("Rust paired devices query failed: \(error)") }
                    }
                }
                continuation.onTermination = { _ in task.cancel() }
            }
        }
        let syncQueue = DispatchQueue(label: "io.local.device.provider.async.updates")

        return AsyncStream { [weak self] continuation in
            guard let self else {
                return
            }

            let uuid = UUID().uuidString
            let cache = asyncProviders
            let provider = subscribeDevicesSnapshot(
                deliverOn: syncQueue,
                update: { continuation.yield($0) },
                failure: { [logger] in logger.error("\($0)") }
            )

            cache.store(value: provider, for: uuid)

            continuation.onTermination = { @Sendable _ in
                cache.clear(for: uuid)
            }
        }
    }
}

private extension LocalDeviceDataProviderFactory {
    func subscribeDevicesSnapshot(
        deliverOn queue: DispatchQueue,
        update: @escaping ([Chat.LocalDevice]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDLocalDevice> = CDLocalDevice.fetchRequest()
        request.sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDLocalDevice.createdAt),
                ascending: false
            )
        ]
        let mapper = LocalDeviceMapper()
        let subscriber = CoreDataSnapshotSubscriber<Chat.LocalDevice, CDLocalDevice>(
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
}
