import Foundation

public struct RuntimeProviderChain: Equatable {
    public let chainId: ChainModel.Id
    public let typesUsage: ChainModel.TypesUsage
    public let name: String
    public let isEthereumBased: Bool

    public init(
        chainId: ChainModel.Id,
        typesUsage: ChainModel.TypesUsage,
        name: String,
        isEthereumBased: Bool
    ) {
        self.chainId = chainId
        self.typesUsage = typesUsage
        self.name = name
        self.isEthereumBased = isEthereumBased
    }
}
