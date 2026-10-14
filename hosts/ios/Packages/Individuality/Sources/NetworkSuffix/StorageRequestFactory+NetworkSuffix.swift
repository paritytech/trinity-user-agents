import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateSdkExt
import StructuredConcurrency

public enum NetworkSuffixError: Error {
    case missing
}

public extension StorageRequestFactoryProtocol {
    /// Reads the network suffix through the same connection and coding factory used for the
    /// surrounding storage reads, ensuring suffix and slot state remain on the same chain.
    func readNetworkSuffix(
        connection: JSONRPCEngine,
        codingFactory: RuntimeCoderFactoryProtocol
    ) async throws -> Data {
        let response: StorageResponse<BytesCodable> = try await queryItem(
            engine: connection,
            factory: { codingFactory },
            storagePath: NetworkSuffixPallet.Storage.networkSuffix(),
            at: nil
        )
        .asyncExecute()

        guard let suffix = response.value?.wrappedValue, !suffix.isEmpty else {
            throw NetworkSuffixError.missing
        }

        return suffix
    }
}
