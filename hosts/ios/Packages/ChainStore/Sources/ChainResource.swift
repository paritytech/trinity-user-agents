import Foundation
import SubstrateSdk

public protocol ChainResourceProtocol: AnyObject {
    func getChainInterface(for chainId: ChainId) -> ChainProtocol?
    func getChainInterfaceByGenesis(_ genesisHash: ChainId) -> ChainProtocol?
    func getRpcConnection(for chainId: ChainId) -> JSONRPCEngine?
    func getRuntimeCodingService(for chainId: ChainId) -> RuntimeCodingServiceProtocol?
}

public extension ChainResourceProtocol {
    func getRpcConnectionOrError(for chainId: ChainId) throws -> JSONRPCEngine {
        guard let connection = getRpcConnection(for: chainId) else {
            throw ChainResourceError.connectionUnavailable
        }

        return connection
    }

    func getRuntimeCodingServiceOrError(for chainId: ChainId) throws -> RuntimeCodingServiceProtocol {
        guard let runtimeProvider = getRuntimeCodingService(for: chainId) else {
            throw ChainResourceError.runtimeMetadaUnavailable
        }

        return runtimeProvider
    }

    func getChainInterfaceOrError(for chainId: ChainId) throws -> ChainProtocol {
        guard let chain = getChainInterface(for: chainId) else {
            throw ChainResourceError.noChain(chainId)
        }

        return chain
    }

    func getChainInterfaceByGenesisOrError(_ genesisHash: ChainId) throws -> ChainProtocol? {
        guard let chain = getChainInterfaceByGenesis(genesisHash) else {
            throw ChainResourceError.noChainGenesis(genesisHash)
        }

        return chain
    }
}
