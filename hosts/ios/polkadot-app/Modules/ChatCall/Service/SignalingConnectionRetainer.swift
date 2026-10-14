import Foundation
import ChainRegistry

actor SignalingConnectionRetainer {
    private let chainRegistry: ChainRegistryProtocol
    private let chainId: ChainModel.Id

    private var token: ConnectionRetainToken?

    init(chainRegistry: ChainRegistryProtocol, chainId: ChainModel.Id) {
        self.chainRegistry = chainRegistry
        self.chainId = chainId
    }

    func update(hasActiveCall: Bool) {
        guard hasActiveCall else {
            token = nil
            return
        }

        guard token == nil else { return }

        guard chainRegistry.getConnection(for: chainId) != nil else { return }

        token = chainRegistry.retainConnections(.chains([chainId]))
    }
}
