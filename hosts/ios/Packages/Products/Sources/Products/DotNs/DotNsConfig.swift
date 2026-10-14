import Foundation
import Revive

public struct DotNsConfig {
    public let contractsChainId: String
    /// Serves names with no registry entry.
    public let resolverContractAddress: EvmAddress
    /// Maps a node to its own resolver. Nil disables manifest resolution.
    public let nameRegistryContractAddress: EvmAddress?
    public let ipfsGatewayBaseUrl: URL

    public init(
        contractsChainId: String,
        resolverContractAddress: EvmAddress,
        nameRegistryContractAddress: EvmAddress?,
        ipfsGatewayBaseUrl: URL
    ) {
        self.contractsChainId = contractsChainId
        self.resolverContractAddress = resolverContractAddress
        self.nameRegistryContractAddress = nameRegistryContractAddress
        self.ipfsGatewayBaseUrl = ipfsGatewayBaseUrl
    }
}
