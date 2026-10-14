import Foundation
import Operation_iOS
import SubstrateSdk

protocol HydraRoutesOperationFactoryProtocol {
    func createRoutesWrapper(
        for swapPair: HydraDx.LocalSwapPair
    ) -> CompoundOperationWrapper<[HydraDx.RemoteSwapRoute]>
}

final class HydraRoutesOperationFactory {
    let omnipoolTokensFactory: HydraOmnipoolTokensFactory
    let stableswapTokensFactory: HydraStableswapTokensFactory
    let xykTokensFactory: HydraXYKPoolTokensFactory
    let runtimeProvider: RuntimeCodingServiceProtocol
    let tokenConverter: HydrationTokenConverting
    let chain: ChainProtocol

    @Atomic(defaultValue: nil)
    private var data: HydraRoutesResolver.Data?

    init(
        chain: ChainProtocol,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        tokenConverter: HydrationTokenConverting,
        operationQueue: OperationQueue
    ) {
        self.chain = chain
        self.runtimeProvider = runtimeProvider
        self.tokenConverter = tokenConverter

        omnipoolTokensFactory = .init(
            chain: chain,
            runtimeService: runtimeProvider,
            connection: connection,
            tokenConverter: tokenConverter,
            operationQueue: operationQueue
        )

        stableswapTokensFactory = .init(
            chain: chain,
            runtimeService: runtimeProvider,
            connection: connection,
            tokenConverter: tokenConverter,
            operationQueue: operationQueue
        )

        xykTokensFactory = .init(
            chain: chain,
            runtimeService: runtimeProvider,
            connection: connection,
            tokenConverter: tokenConverter,
            operationQueue: operationQueue
        )
    }

    private func createDataWrapper() -> CompoundOperationWrapper<HydraRoutesResolver.Data> {
        if let data {
            return CompoundOperationWrapper.createWithResult(data)
        }

        let omnipoolDirectionsWrapper = omnipoolTokensFactory.availableDirections()
        let stableswapDirectionsWrapper = stableswapTokensFactory.availableDirections()
        let stableswapPoolAssetsWrapper = stableswapTokensFactory.fetchAllLocalPoolAssets()
        let xykDirectionsWrapper = xykTokensFactory.availableDirections()

        let resultOperation = ClosureOperation<HydraRoutesResolver.Data> {
            let omnipoolDirections = try omnipoolDirectionsWrapper.targetOperation.extractNoCancellableResultData()
            let stableswapDirections = try stableswapDirectionsWrapper.targetOperation.extractNoCancellableResultData()
            let poolAssets = try stableswapPoolAssetsWrapper.targetOperation.extractNoCancellableResultData()

            let xykDirections = try xykDirectionsWrapper.targetOperation.extractNoCancellableResultData()

            let data = HydraRoutesResolver.Data(
                omnipoolDirections: omnipoolDirections,
                stableswapDirections: stableswapDirections,
                stableswapPoolAssets: poolAssets,
                xykDirections: xykDirections
            )

            self.data = data

            return data
        }

        resultOperation.addDependency(omnipoolDirectionsWrapper.targetOperation)
        resultOperation.addDependency(stableswapDirectionsWrapper.targetOperation)
        resultOperation.addDependency(stableswapPoolAssetsWrapper.targetOperation)
        resultOperation.addDependency(xykDirectionsWrapper.targetOperation)

        let dependencies = omnipoolDirectionsWrapper.allOperations + stableswapDirectionsWrapper.allOperations +
            stableswapPoolAssetsWrapper.allOperations + xykDirectionsWrapper.allOperations

        return CompoundOperationWrapper(targetOperation: resultOperation, dependencies: dependencies)
    }

    private func createRoutesWrapper(
        for swapPair: HydraDx.LocalSwapPair,
        chain: ChainProtocol,
        dataOperation: BaseOperation<HydraRoutesResolver.Data>,
        tokenConverter: HydrationTokenConverting
    ) -> CompoundOperationWrapper<[HydraDx.RemoteSwapRoute]> {
        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

        let resolveOperation = ClosureOperation<[HydraDx.RemoteSwapRoute]> {
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            let data = try dataOperation.extractNoCancellableResultData()

            return HydraRoutesResolver.resolveRoutes(
                for: swapPair,
                data: data,
                chain: chain,
                codingFactory: codingFactory,
                tokenConverter: tokenConverter
            )
        }

        resolveOperation.addDependency(codingFactoryOperation)

        return CompoundOperationWrapper(
            targetOperation: resolveOperation,
            dependencies: [codingFactoryOperation]
        )
    }
}

extension HydraRoutesOperationFactory: HydraRoutesOperationFactoryProtocol {
    func createRoutesWrapper(
        for swapPair: HydraDx.LocalSwapPair
    ) -> CompoundOperationWrapper<[HydraDx.RemoteSwapRoute]> {
        let dataWrapper = createDataWrapper()

        let routesWrapper = createRoutesWrapper(
            for: swapPair,
            chain: chain,
            dataOperation: dataWrapper.targetOperation,
            tokenConverter: tokenConverter
        )

        routesWrapper.addDependency(wrapper: dataWrapper)

        return routesWrapper.insertingHead(operations: dataWrapper.allOperations)
    }
}
