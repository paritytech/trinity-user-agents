import Foundation
import Operation_iOS
import AssetExchange
import SDKLogger

public final class AssetsHydraXYKExchange {
    let host: HydraExchangeHostProtocol
    let tokensFactory: HydraXYKPoolTokensFactory
    let quoteFactory: HydraXYKSwapQuoteFactory
    let logger: SDKLoggerProtocol

    init(
        host: HydraExchangeHostProtocol,
        tokensFactory: HydraXYKPoolTokensFactory,
        quoteFactory: HydraXYKSwapQuoteFactory,
        logger: SDKLoggerProtocol
    ) {
        self.host = host
        self.tokensFactory = tokensFactory
        self.quoteFactory = quoteFactory
        self.logger = logger
    }
}

extension AssetsHydraXYKExchange: AssetsExchangeProtocol {
    public func availableDirectSwapConnections() -> CompoundOperationWrapper<[any AssetExchangableGraphEdge]> {
        let codingFactoryOperation = host.runtimeService.fetchCoderFactoryOperation()
        let remotePairsWrapper = tokensFactory.fetchAllRemotePairsWrapper(dependingOn: codingFactoryOperation)

        remotePairsWrapper.addDependency(operations: [codingFactoryOperation])

        let mappingOperation = ClosureOperation<[any AssetExchangableGraphEdge]> {
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            let remotePairs = try remotePairsWrapper.targetOperation.extractNoCancellableResultData()
            let remoteAssets = Set(remotePairs.values.flatMap { [$0.asset1, $0.asset2] })

            self.logger.debug("Started processing edges")

            let remoteLocalMapping = try self.host.tokenConverting.convertToRemoteLocalMapping(
                remoteAssets: remoteAssets,
                chain: self.host.chain,
                codingFactory: codingFactory
            )

            self.logger.debug("Complete processing edges \(remoteLocalMapping.count)")

            let edges: [AnyAssetExchangeEdge] = remotePairs.values.flatMap { remotePair in
                guard
                    let localAsset1 = remoteLocalMapping[remotePair.asset1],
                    let localAsset2 = remoteLocalMapping[remotePair.asset2] else {
                    return [AnyAssetExchangeEdge]()
                }

                let edge1 = AssetsHydraXYKExchangeEdge(
                    origin: localAsset1,
                    destination: localAsset2,
                    remoteSwapPair: .init(assetIn: remotePair.asset1, assetOut: remotePair.asset2),
                    host: self.host,
                    quoteFactory: self.quoteFactory
                )

                let edge2 = AssetsHydraXYKExchangeEdge(
                    origin: localAsset2,
                    destination: localAsset1,
                    remoteSwapPair: .init(assetIn: remotePair.asset2, assetOut: remotePair.asset1),
                    host: self.host,
                    quoteFactory: self.quoteFactory
                )

                return [AnyAssetExchangeEdge(edge1), AnyAssetExchangeEdge(edge2)]
            }

            return edges
        }

        mappingOperation.addDependency(remotePairsWrapper.targetOperation)

        return remotePairsWrapper
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: mappingOperation)
    }
}
