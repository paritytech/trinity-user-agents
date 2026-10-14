import Foundation
import BigInt
import SubstrateSdk

public enum AssetConversion {
    public enum Direction: Equatable {
        case sell
        case buy
    }

    public struct QuoteArgs: Equatable {
        public let assetIn: ChainAssetId
        public let assetOut: ChainAssetId
        public let amount: BigUInt
        public let direction: Direction

        public init(
            assetIn: ChainAssetId,
            assetOut: ChainAssetId,
            amount: BigUInt,
            direction: Direction
        ) {
            self.assetIn = assetIn
            self.assetOut = assetOut
            self.amount = amount
            self.direction = direction
        }
    }

    public struct Quote: Equatable {
        public let amountIn: BigUInt
        public let assetIn: ChainAssetId
        public let amountOut: BigUInt
        public let assetOut: ChainAssetId
        public let context: String?

        public init(args: QuoteArgs, amount: BigUInt, context: String?) {
            switch args.direction {
            case .sell:
                amountIn = args.amount
                amountOut = amount
            case .buy:
                amountIn = amount
                amountOut = args.amount
            }

            assetIn = args.assetIn
            assetOut = args.assetOut
            self.context = context
        }
    }

    public struct CallArgs: Hashable {
        public let assetIn: ChainAssetId
        public let amountIn: BigUInt
        public let assetOut: ChainAssetId
        public let amountOut: BigUInt
        public let receiver: AccountId
        public let direction: Direction
        public let slippage: BigRational

        public init(
            assetIn: ChainAssetId,
            amountIn: BigUInt,
            assetOut: ChainAssetId,
            amountOut: BigUInt,
            receiver: AccountId,
            direction: Direction,
            slippage: BigRational
        ) {
            self.assetIn = assetIn
            self.amountIn = amountIn
            self.assetOut = assetOut
            self.amountOut = amountOut
            self.receiver = receiver
            self.direction = direction
            self.slippage = slippage
        }
    }
}

public extension AssetConversion.CallArgs {
    var identifier: String { "\(hashValue)" }
}
