import Foundation
import SubstrateSdk

public enum AssetExchangeMetaOperationLabel: Equatable {
    case swap
    case transfer

    var isTransfer: Bool {
        switch self {
        case .transfer:
            true
        case .swap:
            false
        }
    }
}

public protocol AssetExchangeMetaOperationProtocol {
    var assetIn: ChainAssetProtocol { get }
    var assetOut: ChainAssetProtocol { get }
    var amountIn: Balance { get }
    var amountOut: Balance { get }
    var label: AssetExchangeMetaOperationLabel { get }
    var requiresOriginAccountKeepAlive: Bool { get }
}

open class AssetExchangeBaseMetaOperation {
    public let assetIn: ChainAssetProtocol
    public let assetOut: ChainAssetProtocol
    public let amountIn: Balance
    public let amountOut: Balance

    public init(
        assetIn: ChainAssetProtocol,
        assetOut: ChainAssetProtocol,
        amountIn: Balance,
        amountOut: Balance
    ) {
        self.assetIn = assetIn
        self.assetOut = assetOut
        self.amountIn = amountIn
        self.amountOut = amountOut
    }
}
