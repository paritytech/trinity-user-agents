import Foundation
import SubstrateSdk
import AssetExchange

public enum HydraExchange {
    public struct QuoteArgs: Equatable {
        public let assetIn: HydraDx.AssetId
        public let assetOut: HydraDx.AssetId
        public let amount: Balance
        public let direction: AssetConversion.Direction

        public init(
            assetIn: HydraDx.AssetId,
            assetOut: HydraDx.AssetId,
            amount: Balance,
            direction: AssetConversion.Direction
        ) {
            self.assetIn = assetIn
            self.assetOut = assetOut
            self.amount = amount
            self.direction = direction
        }
    }
}
