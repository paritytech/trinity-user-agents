import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency

/// Checks which unload token aliases have been consumed on-chain.
public protocol ConsumedTokenChecking {
    /// For a list of (period, alias) pairs, returns which ones are consumed.
    ///
    /// - Parameters:
    ///   - queries: Array of (period, alias) pairs to check.
    /// - Returns: Array of booleans parallel to `queries`. `true` = consumed, `false` = available.
    func fetchConsumedStatus(for queries: [(period: UInt32, alias: Data)]) async throws -> [Bool]
}

/// Storage query service for `ConsumedFreeUnloadTokens` double map.
public final class ConsumedTokenChecker: ConsumedTokenChecking {
    private let storageRequestFactory: StorageRequestFactory
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol

    public init(
        operationQueue: OperationQueue,
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol
    ) {
        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
        self.connection = connection
        self.runtimeCodingService = runtimeCodingService
    }

    public func fetchConsumedStatus(for queries: [(period: UInt32, alias: Data)]) async throws -> [Bool] {
        let codingFactory = try await runtimeCodingService
            .fetchCoderFactoryOperation()
            .asyncExecute()

        let responses: [StorageResponse<JSON>] = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams1: { queries.map { StringCodable(wrappedValue: $0.period) } },
            keyParams2: { queries.map { BytesCodable(wrappedValue: $0.alias) } },
            factory: { codingFactory },
            storagePath: CoinagePallet.Storage.consumedFreeUnloadTokens()
        )
        .asyncExecute()

        return queries.enumerated().map {
            responses[$0.offset].value != nil
        }
    }
}
