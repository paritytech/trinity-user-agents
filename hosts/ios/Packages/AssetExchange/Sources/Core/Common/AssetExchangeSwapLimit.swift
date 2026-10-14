import Foundation
import SubstrateSdk

public struct AssetExchangeSwapLimit {
    public let direction: AssetConversion.Direction
    public let amountIn: Balance
    public let amountOut: Balance
    public let slippage: BigRational

    public init(
        direction: AssetConversion.Direction,
        amountIn: Balance,
        amountOut: Balance,
        slippage: BigRational
    ) {
        self.direction = direction
        self.amountIn = amountIn
        self.amountOut = amountOut
        self.slippage = slippage
    }
}

private extension AssetExchangeSwapLimit {
    func getNewDirection(for shouldReplaceBuyWithSell: Bool) -> AssetConversion.Direction {
        switch direction {
        case .sell:
            .sell
        case .buy:
            shouldReplaceBuyWithSell ? .sell : .buy
        }
    }
}

public extension AssetExchangeSwapLimit {
    func replacingAmountIn(
        _ newAmountIn: Balance,
        shouldReplaceBuyWithSell: Bool
    ) -> AssetExchangeSwapLimit {
        let newAmountOut = (newAmountIn * amountOut) / amountIn
        let newDirection = getNewDirection(for: shouldReplaceBuyWithSell)

        return AssetExchangeSwapLimit(
            direction: newDirection,
            amountIn: newAmountIn,
            amountOut: newAmountOut,
            slippage: slippage
        )
    }
}
