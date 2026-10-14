import Foundation

public typealias AssetExchangeEdgeType = UInt16

public enum AssetExchangeReservedType: UInt16 {
    case assetHubSwap = 0
    case hydraSwap = 1
    case crossChain = 2
}
