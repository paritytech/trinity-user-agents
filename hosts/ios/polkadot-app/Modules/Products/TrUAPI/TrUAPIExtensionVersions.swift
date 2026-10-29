import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk

/// Builds the ``ExtensionVersionMap`` the rust core queries via `transactionExtensionVersions()`:
/// the chains ``ExtrinsicVersionProvider`` builds as V5 general transactions, each resolved to its
/// genesis hash. A chain the registry cannot resolve fails the whole map, because leaving it out
/// would make the core build V4 transactions for it.
enum TrUAPIExtensionVersions {
    enum Error: Swift.Error, Equatable {
        case unresolvedChain(ChainModel.Id)
        case invalidGenesisHash(ChainModel.Id, String)
    }

    static func make(
        versionProvider: ExtrinsicVersionProvider = ExtrinsicVersionProvider(),
        chainRegistry: ChainRegistryProtocol
    ) throws -> ExtensionVersionMap {
        let versions = versionProvider.generalTransactionExtensionVersions()
        let chains = try versions.map { chainId, extensionVersion in
            let genesis = try genesisHash(of: chainId, chainRegistry: chainRegistry)
            return ChainExtensionVersion(genesisHash: genesis, extensionVersion: extensionVersion)
        }
        return ExtensionVersionMap(chains: chains)
    }
}

private extension TrUAPIExtensionVersions {
    static func genesisHash(
        of chainId: ChainModel.Id,
        chainRegistry: ChainRegistryProtocol
    ) throws -> Data {
        guard let genesisHex = chainRegistry.getChain(for: chainId)?.explicitGenesisHash else {
            throw Error.unresolvedChain(chainId)
        }
        guard let genesis = try? Data(hexString: genesisHex) else {
            throw Error.invalidGenesisHash(chainId, genesisHex)
        }
        return genesis
    }
}
