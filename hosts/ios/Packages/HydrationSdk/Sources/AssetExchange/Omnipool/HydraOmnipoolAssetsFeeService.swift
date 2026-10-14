import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import Operation_iOS
import CommonService
import SDKLogger

final class HydraOmnipoolAssetsFeeService: ObservableSubscriptionSyncService<HydraOmnipool.AssetsFeeState> {
    let chain: ChainProtocol
    let assetIn: HydraDx.AssetId
    let assetOut: HydraDx.AssetId

    init(
        chain: ChainProtocol,
        assetIn: HydraDx.AssetId,
        assetOut: HydraDx.AssetId,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        operationQueue: OperationQueue,
        repository: AnyDataProviderRepository<ChainStorageItem>? = nil,
        workQueue: DispatchQueue = .global(),
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.assetIn = assetIn
        self.assetOut = assetOut

        super.init(
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            repository: repository,
            workQueue: workQueue,
            retryStrategy: retryStrategy,
            logger: logger
        )
    }

    private func getFeeRequest(
        for assetId: HydraDx.AssetId,
        mappingKey: HydraOmnipool.AssetsFeeStateChange.Key
    ) -> BatchStorageSubscriptionRequest {
        .init(
            innerRequest: MapSubscriptionRequest(
                storagePath: HydraDx.dynamicFeesPath,
                localKey: "",
                keyParamClosure: {
                    StringScaleMapper(value: assetId)
                }
            ),
            mappingKey: mappingKey.rawValue
        )
    }

    private func getAssetStateRequest(
        for assetId: HydraDx.AssetId,
        mappingKey: HydraOmnipool.AssetsFeeStateChange.Key
    ) -> BatchStorageSubscriptionRequest {
        .init(
            innerRequest: MapSubscriptionRequest(
                storagePath: HydraOmnipool.assetsPath,
                localKey: "",
                keyParamClosure: { StringScaleMapper(value: assetId) }
            ),
            mappingKey: mappingKey.rawValue
        )
    }

    override func getRequests() throws -> [BatchStorageSubscriptionRequest] {
        let assetInStateRequest = getAssetStateRequest(
            for: assetIn,
            mappingKey: HydraOmnipool.AssetsFeeStateChange.Key.assetInState
        )

        let assetOutStateRequest = getAssetStateRequest(
            for: assetOut,
            mappingKey: HydraOmnipool.AssetsFeeStateChange.Key.assetOutState
        )

        let assetInFeeRequest = getFeeRequest(
            for: assetIn,
            mappingKey: HydraOmnipool.AssetsFeeStateChange.Key.assetInFee
        )

        let assetOutFeeRequest = getFeeRequest(
            for: assetOut,
            mappingKey: HydraOmnipool.AssetsFeeStateChange.Key.assetOutFee
        )

        return [
            assetInStateRequest,
            assetOutStateRequest,
            assetInFeeRequest,
            assetOutFeeRequest
        ]
    }
}
