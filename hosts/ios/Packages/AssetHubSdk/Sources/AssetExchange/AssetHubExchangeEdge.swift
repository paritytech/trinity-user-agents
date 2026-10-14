import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

final class AssetHubExchangeEdge {
    let origin: ChainAssetId
    let destination: ChainAssetId
    let quoteFactory: AssetHubSwapOperationFactoryProtocol
    let host: AssetHubExchangeHostProtocol

    init(
        origin: ChainAssetId,
        destination: ChainAssetId,
        quoteFactory: AssetHubSwapOperationFactoryProtocol,
        host: AssetHubExchangeHostProtocol
    ) {
        self.origin = origin
        self.destination = destination
        self.quoteFactory = quoteFactory
        self.host = host
    }
}

extension AssetHubExchangeEdge: AssetExchangableGraphEdge {
    var type: AssetExchangeEdgeType { AssetExchangeReservedType.assetHubSwap.rawValue }

    var weight: Int { 3 * AssetsExchange.defaultEdgeWeight + 10 }

    func addingWeight(to currentWeight: Int, predecessor _: AnyGraphEdgeProtocol?) -> Int {
        currentWeight + weight
    }

    func quote(
        amount: Balance,
        direction: AssetConversion.Direction
    ) -> CompoundOperationWrapper<Balance> {
        let quoteArgs = AssetConversion.QuoteArgs(
            assetIn: origin,
            assetOut: destination,
            amount: amount,
            direction: direction
        )

        _ = host.flowState.setupReQuoteService()

        let quoteWrapper = quoteFactory.quote(for: quoteArgs)

        let mappingOperation = ClosureOperation<Balance> {
            let quote = try quoteWrapper.targetOperation.extractNoCancellableResultData()
            switch direction {
            case .sell:
                return quote.amountOut
            case .buy:
                return quote.amountIn
            }
        }

        mappingOperation.addDependency(quoteWrapper.targetOperation)

        return quoteWrapper.insertingTail(operation: mappingOperation)
    }

    func beginOperation(for args: AssetExchangeAtomicOperationArgs) throws -> AssetExchangeAtomicOperationProtocol {
        AssetHubExchangeAtomicOperation(
            host: host,
            operationArgs: args,
            edge: self
        )
    }

    func appendToOperation(
        _: AssetExchangeAtomicOperationProtocol,
        args _: AssetExchangeAtomicOperationArgs
    ) -> AssetExchangeAtomicOperationProtocol? {
        nil
    }

    func shouldIgnoreFeeRequirement(after _: any AssetExchangableGraphEdge) -> Bool {
        false
    }

    func shouldIgnoreDelayedCallRequirement(after _: any AssetExchangableGraphEdge) -> Bool {
        false
    }

    func canPayNonNativeFeesInIntermediatePosition() -> Bool {
        true
    }

    func requiresOriginKeepAliveOnIntermediatePosition() -> Bool {
        false
    }

    func beginMetaOperation(
        for amountIn: Balance,
        amountOut: Balance
    ) throws -> AssetExchangeMetaOperationProtocol {
        let assetIn = try host.chain.chainAssetInterfaceOrError(
            for: origin.assetId
        )

        let assetOut = try host.chain.chainAssetInterfaceOrError(
            for: destination.assetId
        )

        return AssetHubExchangeMetaOperation(
            assetIn: assetIn,
            assetOut: assetOut,
            amountIn: amountIn,
            amountOut: amountOut
        )
    }

    func appendToMetaOperation(
        _: AssetExchangeMetaOperationProtocol,
        amountIn _: Balance,
        amountOut _: Balance
    ) throws -> AssetExchangeMetaOperationProtocol? {
        nil
    }

    func beginOperationPrototype() throws -> AssetExchangeOperationPrototypeProtocol {
        let assetIn = try host.chain.chainAssetInterfaceOrError(
            for: origin.assetId
        )

        let assetOut = try host.chain.chainAssetInterfaceOrError(
            for: destination.assetId
        )

        return AssetHubExchangeOperationPrototype(assetIn: assetIn, assetOut: assetOut, host: host)
    }

    func appendToOperationPrototype(
        _: AssetExchangeOperationPrototypeProtocol
    ) throws -> AssetExchangeOperationPrototypeProtocol? {
        nil
    }
}
