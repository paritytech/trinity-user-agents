import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency
import Individuality

/// Provable ring capacity (max anonymity-set size) per voucher denomination. Capacity is a pure
/// function of the recycler collection's ring exponent, which is chain config and cannot change
/// within a session, so results are memoised per denomination.
public protocol RingCapacityProviding: Sendable {
    func capacities(for exponents: Set<Int16>) async throws -> [Int16: Int]

    /// Only the capacities already memoised — no chain read. Used by the ``BalanceEvaluationMode/immediate``
    /// pass, which must not block; an unresolved denomination simply reads as not-yet-full.
    func peekCapacities(for exponents: Set<Int16>) async -> [Int16: Int]
}

public actor RingCapacityProvider: RingCapacityProviding {
    private let instanceId: CoinageInstanceId
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol
    private let requestFactory: StorageRequestFactory

    private var cache: [Int16: Int] = [:]

    public init(
        instanceId: CoinageInstanceId,
        operationQueue: OperationQueue,
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol
    ) {
        self.instanceId = instanceId
        self.connection = connection
        self.runtimeCodingService = runtimeCodingService
        requestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }

    public func capacities(for exponents: Set<Int16>) async throws -> [Int16: Int] {
        let missing = exponents.subtracting(cache.keys)

        guard !missing.isEmpty else {
            return cache.filter { exponents.contains($0.key) }
        }

        let codingFactory = try await runtimeCodingService.fetchCoderFactoryOperation().asyncExecute()

        for exponent in missing {
            let collectionId = RecyclerCollectionIdentifier.identifier(instanceId: instanceId, for: exponent)

            let responses: [StorageResponse<MembersPallet.CollectionInfo>] = try await requestFactory.queryItems(
                engine: connection,
                keyParams: { [BytesCodable(wrappedValue: collectionId)] },
                factory: { codingFactory },
                storagePath: MembersPallet.Storage.collections(),
                at: nil
            )
            .asyncExecute()

            if let capacity = responses.first?.value?.ringSize.ringCapacity {
                cache[exponent] = capacity
            }
        }

        return cache.filter { exponents.contains($0.key) }
    }

    public func peekCapacities(for exponents: Set<Int16>) async -> [Int16: Int] {
        cache.filter { exponents.contains($0.key) }
    }
}
