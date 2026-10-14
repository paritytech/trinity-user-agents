import ChainStore
import Foundation
import SubstrateSdk

final class StubChainResource: ChainResourceProtocol {
    let chainId: ChainId
    let connection: JSONRPCEngine
    let runtimeService: RuntimeCodingServiceProtocol

    init(chainId: ChainId, connection: JSONRPCEngine, runtimeService: RuntimeCodingServiceProtocol) {
        self.chainId = chainId
        self.connection = connection
        self.runtimeService = runtimeService
    }

    func getChainInterface(for _: ChainId) -> ChainProtocol? { nil }
    func getChainInterfaceByGenesis(_: ChainId) -> ChainProtocol? { nil }

    func getRpcConnection(for chainId: ChainId) -> JSONRPCEngine? {
        chainId == self.chainId ? connection : nil
    }

    func getRuntimeCodingService(for chainId: ChainId) -> RuntimeCodingServiceProtocol? {
        chainId == self.chainId ? runtimeService : nil
    }
}
