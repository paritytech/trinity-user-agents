import Foundation
import Products
import ChainRegistry
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery

enum DotNsTldError: Error {
    case unavailable
}

extension DotNsTldProviding {
    /// Synchronous TLD access for post-onboarding derivation. The TLD is immutable once the DotNs
    /// contract is deployed, so a completed onboarding guarantees a cached value; throws otherwise
    /// rather than deriving a built-in account against an unknown TLD.
    func currentTldOrError() throws -> String {
        guard let tld = currentTld() else { throw DotNsTldError.unavailable }
        return tld
    }
}

/// Single process-wide TLD provider. The chain config is read lazily at chain-call time, so the
/// instance can be constructed before remote config loads without blocking or crashing.
enum DotNsTldProviderFacade {
    static let shared: DotNsTldProviding = DotNsTldProvider(
        reader: NetworkSuffixTldReader(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            storageRequestFactory: StorageRequestFactory(
                remoteFactory: StorageKeyFactory(),
                operationManager: OperationManager(
                    operationQueue: OperationManagerFacade.sharedDefaultQueue
                )
            ),
            configProvider: { try AppConfig.DotNs.config() }
        ),
        store: SettingsDotNsTldStore(),
        logger: Logger.shared
    )
}
