import Foundation
import Operation_iOS
import ChainRegistry

protocol RemoteConfigManaging: AnyObject {
    func fetchRemoteConfigValues()
    func asyncWaitChainsForRemoteConfigValues() -> CompoundOperationWrapper<[RemoteChainModel]>
    func asyncWaitXcmTransfers<T: Decodable>() -> CompoundOperationWrapper<T>
    func asyncWaitXcmGeneralConfig<T: Decodable>() -> CompoundOperationWrapper<T>
    func asyncWaitW3sMerchants<T: Decodable>() -> CompoundOperationWrapper<T>

    /// Per-chain transaction-extension version, keyed by chain id, from the standalone
    /// `transaction_extension_versions` remote-config key.
    func syncedTxExtensionVersions() -> [ChainModel.Id: UInt8]

    func asyncWaitRemoteConfig() async throws -> RemoteAppConfig
}

/// Registry wiring that only the facade owns, kept off RemoteConfigManaging because that protocol's other conformer has
/// no registry.
protocol ChainRegistryConfiguring: AnyObject {
    func set(chainRegistry: ChainRegistryProtocol)
}

/// Lets an in-process restart wait for a config fetched after it, instead of resolving with the one already applied
/// or the one Firebase activated earlier.
protocol AppliedConfigDiscarding: AnyObject {
    /// Until the next successful fetch, waiters stay pending and the activated cache is not applied. The values
    /// applied so far stay readable, so code still running against them does not trap.
    @MainActor
    func discardAppliedConfig()
}
