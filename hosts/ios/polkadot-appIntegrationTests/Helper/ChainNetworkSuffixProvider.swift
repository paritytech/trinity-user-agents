import ChainRegistry
import ChainStore
import Foundation
import Individuality
import StructuredConcurrency
import SubstrateSdk
@preconcurrency import SubstrateStorageQuery

/// Reads the suffix straight from the chain under test, which has no cached TLD to take it from.
struct ChainNetworkSuffixProvider: NetworkSuffixProviding, @unchecked Sendable {
    let chainId: ChainModel.Id
    let chainRegistry: ChainRegistryProtocol
    let storageRequestFactory: StorageRequestFactoryProtocol

    func networkSuffix() async throws -> Data {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        return try await storageRequestFactory.readNetworkSuffix(connection: connection, codingFactory: codingFactory)
    }
}
