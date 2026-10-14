import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import ChainStore
import FoundationExt

public protocol ChainTimeProviding {
    func nowSeconds(at blockHash: Data?) async throws -> UInt64
}

public extension ChainTimeProviding {
    func nowSeconds() async throws -> UInt64 {
        try await nowSeconds(at: nil)
    }

    func currentPeriod() async throws -> UInt32 {
        try await UInt32(TimeInterval(nowSeconds()) / .secondsInDay)
    }
}

enum ChainTimeProviderError: Error {
    case timestampUnavailable
}

public final class ChainTimeProvider: ChainTimeProviding {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
    }

    public func nowSeconds(at blockHash: Data?) async throws -> UInt64 {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let response: StorageResponse<StringScaleMapper<UInt64>> = try await storageRequestFactory.queryItem(
            engine: connection,
            factory: { codingFactory },
            storagePath: TimestampPallet.Storage.now(),
            at: blockHash
        )
        .asyncExecute()

        guard let millis = response.value?.value else {
            throw ChainTimeProviderError.timestampUnavailable
        }
        return millis / 1_000
    }
}
