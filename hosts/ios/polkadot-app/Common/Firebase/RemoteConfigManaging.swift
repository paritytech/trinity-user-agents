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
