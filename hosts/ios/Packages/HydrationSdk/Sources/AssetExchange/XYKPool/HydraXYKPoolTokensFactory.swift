import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery

final class HydraXYKPoolTokensFactory {
    let chain: ChainProtocol
    let operationQueue: OperationQueue
    let runtimeService: RuntimeCodingServiceProtocol
    let connection: JSONRPCEngine
    let tokenConverter: HydrationTokenConverting

    init(
        chain: ChainProtocol,
        runtimeService: RuntimeCodingServiceProtocol,
        connection: JSONRPCEngine,
        tokenConverter: HydrationTokenConverting,
        operationQueue: OperationQueue
    ) {
        self.chain = chain
        self.runtimeService = runtimeService
        self.connection = connection
        self.tokenConverter = tokenConverter
        self.operationQueue = operationQueue
    }

    func fetchAllRemotePairsWrapper(
        dependingOn codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>
    ) -> CompoundOperationWrapper<[AccountIdKey: HydraXYK.PoolAssets]> {
        let requestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let request = UnkeyedRemoteStorageRequest(storagePath: HydraXYK.poolAssetsPath)

        let fetchWrapper: CompoundOperationWrapper<[AccountIdKey: HydraXYK.PoolAssets]>
        fetchWrapper = requestFactory.queryByPrefix(
            engine: connection,
            request: request,
            storagePath: HydraXYK.poolAssetsPath,
            factory: { try codingFactoryOperation.extractNoCancellableResultData() }
        )

        return fetchWrapper
    }

    private func fetchAllDirections(
        for chain: ChainProtocol,
        tokenConverter: HydrationTokenConverting
    ) -> CompoundOperationWrapper<[ChainAssetId: Set<ChainAssetId>]> {
        let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()
        let fetchRemotePairs = fetchAllRemotePairsWrapper(dependingOn: codingFactoryOperation)

        fetchRemotePairs.addDependency(operations: [codingFactoryOperation])

        let conversionOperation = ClosureOperation<[ChainAssetId: Set<ChainAssetId>]> {
            let pairs = try fetchRemotePairs.targetOperation.extractNoCancellableResultData()
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            let localAssets = chain.chainAssetsInterface()
            let remoteToLocal = try localAssets.reduce(into: [HydraDx.AssetId: ChainAssetId]()) { accum, chainAsset in
                let remoteId = try tokenConverter.convertToRemote(
                    chainAsset: chainAsset,
                    codingFactory: codingFactory
                ).remoteAssetId

                accum[remoteId] = chainAsset.chainAssetId
            }

            return pairs.reduce(into: [:]) { accum, remotePair in
                guard
                    let localAsset1 = remoteToLocal[remotePair.value.asset1],
                    let localAsset2 = remoteToLocal[remotePair.value.asset2] else {
                    return
                }

                accum[localAsset1] = accum[localAsset1]?.union([localAsset2]) ??
                    [localAsset2]
                accum[localAsset2] = accum[localAsset2]?.union([localAsset1]) ??
                    [localAsset1]
            }
        }

        conversionOperation.addDependency(fetchRemotePairs.targetOperation)

        return fetchRemotePairs
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: conversionOperation)
    }
}

extension HydraXYKPoolTokensFactory: HydraPoolTokensFactoryProtocol {
    func availableDirections() -> CompoundOperationWrapper<[ChainAssetId: Set<ChainAssetId>]> {
        fetchAllDirections(for: chain, tokenConverter: tokenConverter)
    }

    func availableDirectionsForAsset(
        _ chainAssetId: ChainAssetId
    ) -> CompoundOperationWrapper<Set<ChainAssetId>> {
        let allDirectionsWrapper = fetchAllDirections(for: chain, tokenConverter: tokenConverter)

        let mappingOperation = ClosureOperation<Set<ChainAssetId>> {
            let allDirections = try allDirectionsWrapper.targetOperation.extractNoCancellableResultData()

            return allDirections[chainAssetId] ?? Set()
        }

        mappingOperation.addDependency(allDirectionsWrapper.targetOperation)

        return allDirectionsWrapper.insertingTail(operation: mappingOperation)
    }
}
