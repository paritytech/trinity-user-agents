import Foundation
import Operation_iOS
import SubstrateSdk
import ChainStore
import AssetExchange

final class CrosschainExchangeEdge {
    let origin: ChainAssetId
    let destination: ChainAssetId
    let host: CrosschainExchangeHostProtocol
    let features: XcmTransferFeatures

    init(
        origin: ChainAssetId,
        destination: ChainAssetId,
        host: CrosschainExchangeHostProtocol,
        features: XcmTransferFeatures
    ) {
        self.origin = origin
        self.destination = destination
        self.host = host
        self.features = features
    }

    private func deliveryFeeNotPaidOrFromHolding() -> Bool {
        // xcm execute allows to pay delivery fee from holding
        !features.hasDeliveryFee || features.shouldUseXcmExecute
    }

    private func shouldProhibitTransferOutAll() -> Bool {
        host.fungibilityPreservationProvider.requiresPreservationForCrosschain(
            assetIn: origin,
            features: features
        )
    }
}

extension CrosschainExchangeEdge: AssetExchangableGraphEdge {
    var type: AssetExchangeEdgeType { AssetExchangeReservedType.crossChain.rawValue }

    var weight: Int { AssetsExchange.defaultEdgeWeight }

    func addingWeight(to currentWeight: Int, predecessor _: AnyGraphEdgeProtocol?) -> Int {
        currentWeight + weight
    }

    func quote(
        amount: Balance,
        direction _: AssetConversion.Direction
    ) -> CompoundOperationWrapper<Balance> {
        CompoundOperationWrapper.createWithResult(amount)
    }

    func beginOperation(for args: AssetExchangeAtomicOperationArgs) throws -> AssetExchangeAtomicOperationProtocol {
        CrosschainExchangeAtomicOperation(
            host: host,
            edge: self,
            operationArgs: args
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
        deliveryFeeNotPaidOrFromHolding()
    }

    func requiresOriginKeepAliveOnIntermediatePosition() -> Bool {
        shouldProhibitTransferOutAll()
    }

    func beginMetaOperation(
        for amountIn: Balance,
        amountOut: Balance
    ) throws -> AssetExchangeMetaOperationProtocol {
        guard let chainIn = host.allChains[origin.chainId] else {
            throw ChainResourceError.noChain(origin.chainId)
        }

        guard let chainOut = host.allChains[destination.chainId] else {
            throw ChainResourceError.noChain(destination.chainId)
        }

        let assetIn = try chainIn.chainAssetInterfaceOrError(for: origin.assetId)

        let assetOut = try chainOut.chainAssetInterfaceOrError(for: destination.assetId)

        let keepAlive = shouldProhibitTransferOutAll()

        return CrosschainExchangeMetaOperation(
            assetIn: assetIn,
            assetOut: assetOut,
            amountIn: amountIn,
            amountOut: amountOut,
            requiresOriginAccountKeepAlive: keepAlive
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
        guard let chainIn = host.allChains[origin.chainId] else {
            throw ChainResourceError.noChain(origin.chainId)
        }

        guard let chainOut = host.allChains[destination.chainId] else {
            throw ChainResourceError.noChain(destination.chainId)
        }

        let assetIn = try chainIn.chainAssetInterfaceOrError(for: origin.assetId)

        let assetOut = try chainOut.chainAssetInterfaceOrError(for: destination.assetId)

        return CrosschainExchangeOperationPrototype(assetIn: assetIn, assetOut: assetOut, host: host)
    }

    func appendToOperationPrototype(
        _: AssetExchangeOperationPrototypeProtocol
    ) throws -> AssetExchangeOperationPrototypeProtocol? {
        nil
    }
}
