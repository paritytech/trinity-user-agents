import Foundation
import Operation_iOS
@preconcurrency import SubstrateSdk
@preconcurrency import SubstrateStorageQuery
import SubstrateStorageSubscription
import StructuredConcurrency
import Individuality
import AsyncExtensions

/// Protocol for loading recycler states from chain.
public protocol RecyclerReadinessLoading: Sendable {
    /// Fetches the current revisions from Members.Root storage.
    func fetchRevisions(
        for keys: [RecyclerKey],
        blockHash: BlockHashData?
    ) async throws -> [RecyclerKey: UInt32]

    /// Fetches maximum amount of vouchers to be unloaded
    func maxConsolidation() async throws -> UInt32

    /// Fetches the maximum number of coins a single unload call may mint.
    func maxSplitOutputs() async throws -> UInt32
}

/// Loader for querying Members.RingKeysStatus storage with async/await API.
public final class RecyclerReadinessLoader: RecyclerReadinessLoading {
    private let instanceId: CoinageInstanceId
    private let storageRequestFactory: StorageRequestFactory
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol

    public init(
        instanceId: CoinageInstanceId,
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol,
        operationQueue: OperationQueue
    ) {
        self.instanceId = instanceId
        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
        self.runtimeCodingService = runtimeCodingService
        self.connection = connection
    }

    public func fetchRevisions(
        for keys: [RecyclerKey],
        blockHash: BlockHashData?
    ) async throws -> [RecyclerKey: UInt32] {
        let codingFactory = try await runtimeCodingService
            .fetchCoderFactoryOperation()
            .asyncExecute()

        let identifiers = keys.map { BytesCodable(wrappedValue: $0.identifier(instanceId: instanceId)) }
        let ringIndices = keys.map { StringCodable(wrappedValue: $0.index) }

        let options = blockHash.map { StorageQueryListOptions(atBlock: $0) } ?? StorageQueryListOptions()

        let storageResponses: [StorageResponse<MembersPallet.RingRoot>] =
            try await storageRequestFactory.queryItems(
                engine: connection,
                keyParams1: { identifiers },
                keyParams2: { ringIndices },
                factory: { codingFactory },
                storagePath: MembersPallet.Storage.root(),
                options: options
            ).asyncExecute()

        return zip(keys, storageResponses).reduce(into: [:]) { acc, pair in
            guard let root = pair.1.value else { return }
            acc[pair.0] = root.revision
        }
    }

    public func maxConsolidation() async throws -> UInt32 {
        try await runtimeCodingService.fetchConstant(
            path: CoinagePallet.Constants.maxConsolidation(),
            type: UInt32.self
        )
    }

    public func maxSplitOutputs() async throws -> UInt32 {
        try await runtimeCodingService.fetchConstant(
            path: CoinagePallet.Constants.maxSplitOutputs(),
            type: UInt32.self
        )
    }
}

private extension RecyclerKey {
    func identifier(instanceId: CoinageInstanceId) -> Data {
        RecyclerCollectionIdentifier.identifier(instanceId: instanceId, for: exponent)
    }
}
