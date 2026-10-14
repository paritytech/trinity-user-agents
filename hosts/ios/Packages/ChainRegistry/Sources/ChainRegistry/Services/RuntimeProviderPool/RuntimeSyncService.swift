import Foundation
import Operation_iOS
import SubstrateSdk
import SDKLogger
import SubstrateSdkExt
import EventCenter

public protocol RuntimeSyncServiceProtocol {
    func register(chain: ChainModel, with connection: ChainConnection)
    func unregisterIfExists(chainId: ChainModel.Id)
    func apply(version: RuntimeVersion, for chainId: ChainModel.Id)

    func hasChain(with chainId: ChainModel.Id) -> Bool
    func isChainSyncing(_ chainId: ChainModel.Id) -> Bool
}

public enum RuntimeSyncServiceError: Error {
    case skipMetadataUnchanged
}

public final class RuntimeSyncService {
    public struct SyncInfo {
        let typesURL: URL?
        let connection: JSONRPCEngine
    }

    public struct SyncResult {
        let chainId: ChainModel.Id
        let typesSyncResult: Result<String, Error>?
        let metadataSyncResult: Result<Void, Error>?
        let runtimeVersion: RuntimeVersion?
    }

    public struct RetryAttempt {
        let chainId: ChainModel.Id
        let shouldSyncTypes: Bool
        let runtimeVersion: RuntimeVersion?
        let attempt: Int
    }

    public let repository: AnyDataProviderRepository<RuntimeMetadataItem>
    public let filesOperationFactory: RuntimeFilesOperationFactoryProtocol
    public let dataOperationFactory: DataOperationFactoryProtocol
    public let runtimeFetchFactory: RuntimeFetchOperationFactoryProtocol
    public let runtimeLocalMigrator: RuntimeLocalMigrating
    public let eventCenter: EventCenterProtocol
    public let retryStrategy: ReconnectionStrategyProtocol
    public let operationQueue: OperationQueue
    public let dataHasher: StorageHasher
    public let logger: SDKLoggerProtocol?
    public let rpcTimeout: Int

    public var knownChains: [ChainModel.Id: SyncInfo] = [:]
    public var syncingChains: [ChainModel.Id: CompoundOperationWrapper<SyncResult>] = [:]
    public var retryAttempts: [ChainModel.Id: RetryAttempt] = [:]
    public var mutex = NSLock()
    public var retryScheduler: Scheduler?

    public init(
        repository: AnyDataProviderRepository<RuntimeMetadataItem>,
        runtimeFetchFactory: RuntimeFetchOperationFactoryProtocol,
        runtimeLocalMigrator: RuntimeLocalMigrating,
        filesOperationFactory: RuntimeFilesOperationFactoryProtocol,
        dataOperationFactory: DataOperationFactoryProtocol,
        eventCenter: EventCenterProtocol,
        operationQueue: OperationQueue,
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        dataHasher: StorageHasher = .twox256,
        rpcTimeout: Int = Int(UInt16.max),
        logger: SDKLoggerProtocol? = nil
    ) {
        self.repository = repository
        self.runtimeFetchFactory = runtimeFetchFactory
        self.runtimeLocalMigrator = runtimeLocalMigrator
        self.filesOperationFactory = filesOperationFactory
        self.dataOperationFactory = dataOperationFactory
        self.retryStrategy = retryStrategy
        self.eventCenter = eventCenter
        self.dataHasher = dataHasher
        self.rpcTimeout = rpcTimeout
        self.logger = logger
        self.operationQueue = operationQueue
    }

    public func clearOperations(for chainId: ChainModel.Id) {
        if let existingOperation = syncingChains[chainId] {
            syncingChains[chainId] = nil
            existingOperation.cancel()
        }

        retryAttempts[chainId] = nil
    }
}

extension RuntimeSyncService: SchedulerDelegate {
    public func didTrigger(scheduler _: SchedulerProtocol) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        retryScheduler = nil

        for requestKeyValue in retryAttempts where syncingChains[requestKeyValue.key] == nil {
            performSync(
                for: requestKeyValue.key,
                shouldSyncTypes: requestKeyValue.value.shouldSyncTypes,
                newVersion: requestKeyValue.value.runtimeVersion
            )
        }
    }
}
