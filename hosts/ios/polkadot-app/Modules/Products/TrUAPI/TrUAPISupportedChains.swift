import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk

/// Builds the ``HostChainSet`` the rust core queries via `supportedChains()`:
/// the app's configured people, bulletin, and asset-hub chains, each resolved
/// to its genesis hash. Chains the registry cannot resolve are omitted so the
/// set matches exactly what `chainConnect` accepts.
enum TrUAPISupportedChains {
    static func make(chainRegistry: ChainRegistryProtocol) -> HostChainSet {
        let roles: [(identifier: ChainIdentifier, chainId: ChainModel.Id)] = [
            (.people, AppConfig.Chains.usernameChain),
            (.bulletin, AppConfig.Chains.bulletInChain),
            (.assetHub, AppConfig.Chains.assethubChain)
        ]

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
