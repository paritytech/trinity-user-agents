import Foundation
import SubstrateSdk

public struct AssetExchangeRoute: Equatable {
    public let items: [AssetExchangeRouteItem]
    public let amount: Balance
    public let direction: AssetConversion.Direction

    public var quote: Balance {
        switch direction {
        case .sell:
            items.last?.quote ?? amount
        case .buy:
            items.first?.quote ?? amount
        }
    }

    public var amountIn: Balance {
        items.first?.amountIn(for: direction) ?? amount
    }

    public var amountOut: Balance {
        items.last?.amountOut(for: direction) ?? amount
    }

    public func byAddingNext(item: AssetExchangeRouteItem) -> AssetExchangeRoute {
        switch direction {
        case .sell:
            .init(items: items + [item], amount: amount, direction: direction)
        case .buy:
            .init(items: [item] + items, amount: amount, direction: direction)
        }
    }

    public func matches(otherRoute: AssetExchangeRoute, slippage: BigRational) -> Bool {
        guard direction == otherRoute.direction else { return false }

        switch direction {
        case .sell:
            let amountOutMin = amountOut - slippage.mul(value: amountOut)

            return amountOutMin <= otherRoute.amountOut
        case .buy:
            let amountInMax = amountIn + slippage.mul(value: amountIn)

            return amountInMax >= otherRoute.amountIn
        }
    }
}

public extension AssetExchangeGraphPath {
    func quoteIteration(for direction: AssetConversion.Direction) -> AssetExchangeGraphPath {
        switch direction {
        case .sell:
            self
        case .buy:
            AssetExchangeGraphPath(reversed())
        }
    }
}

public struct AssetExchangeRouteItem {
    public let edge: AnyAssetExchangeEdge
    public let amount: Balance
    public let quote: Balance

    public func amountIn(for direction: AssetConversion.Direction) -> Balance {
        switch direction {
        case .sell:
            amount
        case .buy:
            quote
        }
    }

    public func amountOut(for direction: AssetConversion.Direction) -> Balance {
        switch direction {
        case .sell:
            quote
        case .buy:
            amount
        }
    }
}

extension AssetExchangeRouteItem: Equatable {
    public static func == (lhs: AssetExchangeRouteItem, rhs: AssetExchangeRouteItem) -> Bool {
        lhs.edge.identifier == rhs.edge.identifier &&
            lhs.amount == rhs.amount &&
            lhs.quote == rhs.quote
    }
}
