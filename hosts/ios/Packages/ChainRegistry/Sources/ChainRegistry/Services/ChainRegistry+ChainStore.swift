import Foundation
import ChainStore
import SubstrateSdk

public extension ChainRegistryProtocol {
    func getRpcConnection(for chainId: ChainId) -> JSONRPCEngine? {
        getConnection(for: chainId)
    }

    func getRuntimeCodingService(for chainId: ChainId) -> RuntimeCodingServiceProtocol? {
        getRuntimeProvider(for: chainId)
    }

    func getChainInterface(for chainId: ChainId) -> ChainProtocol? {
        getChain(for: chainId)
    }

    func getChainInterfaceByGenesis(_ genesisHash: ChainId) -> ChainProtocol? {
        getChainByGenesis(for: genesisHash)
    }
}
