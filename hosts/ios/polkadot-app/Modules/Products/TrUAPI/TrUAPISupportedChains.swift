import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk

/// Builds the ``HostChainSet`` the rust core queries via `supportedChains()`:
/// the app's configured people, bulletin, and asset-hub chains, each resolved
/// to its genesis hash. Chains the registry cannot resolve are omitted so the
/// set matches exactly what `chainConnect` accepts.
enum TrUAPISupportedChains {
    private struct Role {
        let identifier: ChainIdentifier
        let chainId: ChainModel.Id
        /// How host telemetry names the chain, as the Android host does.
        let telemetryLabel: String
    }

    private static var roles: [Role] {
        [
            Role(identifier: .people, chainId: AppConfig.Chains.peopleChain, telemetryLabel: "people"),
            Role(identifier: .bulletin, chainId: AppConfig.Chains.bulletInChain, telemetryLabel: "bulletin"),
            Role(identifier: .assetHub, chainId: AppConfig.Chains.assethubChain, telemetryLabel: "asset_hub")
        ]
    }

    static func make(chainRegistry: ChainRegistryProtocol) -> HostChainSet {
        let chains = roles.compactMap { role -> HostChainEntry? in
            guard
                let chain = chainRegistry.getChain(for: role.chainId),
                let genesisHex = chain.explicitGenesisHash,
                let genesis = try? Data(hexString: genesisHex)
            else {
                return nil
            }
            return HostChainEntry(identifier: role.identifier, genesisHash: genesis)
        }

        return HostChainSet(network: network, chains: chains)
    }

    static func telemetryLabel(for chainId: ChainModel.Id) -> String? {
        roles.first { $0.chainId == chainId }?.telemetryLabel
    }

    private static var network: String {
        #if UNSTABLE
            "polkadot"
        #elseif SAFETYNET
            "polkadot"
        #elseif NIGHTLY
            "paseo"
        #else
            "polkadot"
        #endif
    }
}
