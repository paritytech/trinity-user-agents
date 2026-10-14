import BigInt
import ChainStore
import Foundation
import HydrationSdk
import SubstrateSdk
import StructuredConcurrency

struct HydrationEvmBalanceHandler {
    let chainResource: ChainResourceProtocol
    let operationQueue: OperationQueue

    func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        remoteAssetId: BigUInt
    ) async throws -> AssetBalance {
        let runtimeConnectionStore = ChainRegistryRuntimeConnectionStore(
            chainId: chainAssetId.chainId,
            chainRegistry: chainResource
        )

        let apiFactory = HydrationApiOperationFactory(
            runtimeConnectionStore: runtimeConnectionStore,
            operationQueue: operationQueue
        )

        let currencyData = try await apiFactory.createCurrencyBalanceWrapper(
            for: { remoteAssetId },
            accountId: accountId,
            blockHash: nil
        ).asyncExecute()

        return AssetBalance(
            hydrationCurrencyData: currencyData,
            chainAssetId: chainAssetId,
            accountId: accountId
        )
    }
}
