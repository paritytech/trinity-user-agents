import Foundation
import Foundation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import Operation_iOS
import CommonService
import SDKLogger

final class HydraStableswapReservesService: ObservableSubscriptionSyncService<HydraStableswap.ReservesRemoteState> {
    let poolAsset: HydraDx.AssetId

    init(
        poolAsset: HydraDx.AssetId,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        operationQueue: OperationQueue,
        repository: AnyDataProviderRepository<ChainStorageItem>? = nil,
        workQueue: DispatchQueue = .global(),
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        logger: SDKLoggerProtocol
    ) {
        self.poolAsset = poolAsset

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

    func getRequests(for poolAsset: HydraDx.AssetId) throws -> [BatchStorageSubscriptionRequest] {
        let poolAssetTotalIssuanceRequest = BatchStorageSubscriptionRequest(
            innerRequest: MapSubscriptionRequest(
                storagePath: OrmlPallet.ormlTotalIssuance,
                localKey: "",
                keyParamClosure: {
                    StringScaleMapper(value: poolAsset)
                }
            ),
            mappingKey: HydraStableswap.ReservesRemoteStateChange.Key.poolIssuance.rawValue
        )

        return [poolAssetTotalIssuanceRequest]
    }

    override func getRequests() throws -> [BatchStorageSubscriptionRequest] {
        try getRequests(for: poolAsset)
    }
}
