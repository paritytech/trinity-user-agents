import Foundation

public protocol ChainNodeConnectable {
    var chainId: String { get }
    var name: String { get }
    var nodes: Set<ChainNodeModel> { get }
    var options: [LocalChainOptions]? { get }
    var nodeSwitchStrategy: ChainModel.NodeSwitchStrategy { get }
    var addressPrefix: ChainModel.AddressPrefix { get }
}

public extension ChainNodeConnectable {
    var noSubstrateRuntime: Bool {
        options?.contains(where: { $0 == .noSubstrateRuntime }) ?? false
    }

    var hasSubstrateRuntime: Bool {
        !noSubstrateRuntime
    }

    var isEthereumBased: Bool {
        options?.contains(.ethereumBased) ?? false
    }
}

extension ChainModel: ChainNodeConnectable {}
