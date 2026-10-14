import Foundation

public struct AssetExchangeQuote {
    public let route: AssetExchangeRoute
    public let metaOperations: [AssetExchangeMetaOperationProtocol]
    public let executionTimes: [TimeInterval]

    public func totalExecutionTime() -> TimeInterval {
        executionTimes.reduce(0, +)
    }

    public func totalExecutionTime(from index: Int) -> TimeInterval {
        let subarray =
            if index > 0 {
                Array(executionTimes.dropFirst(index))
            } else {
                executionTimes
            }

        return subarray.reduce(0, +)
    }

    public func hasSamePath(other: AssetExchangeRoute) -> Bool {
        guard route.items.count == other.items.count else { return false }

        return zip(route.items, other.items).allSatisfy {
            $0.0.edge.identifier == $0.1.edge.identifier
        }
    }
}
