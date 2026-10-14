import ChainStore
import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency
import Operation_iOS

struct NativeBalanceHandler {
    let chainResource: ChainResourceProtocol
    let operationQueue: OperationQueue

    func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId
    ) async throws -> AssetBalance {
        let runtimeProvider = try chainResource.getRuntimeCodingServiceOrError(
            for: chainAssetId.chainId
        )
        let connection = try chainResource.getRpcConnectionOrError(
            for: chainAssetId.chainId
        )

        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let requestFactory = StorageRequestFactory.asyncInit()

        let responses: [StorageResponse<SystemPallet.AccountInfo>] = try await requestFactory.queryItems(
            engine: connection,
            keyParams: { [BytesCodable(wrappedValue: accountId)] },
            factory: { codingFactory },
            storagePath: SystemPallet.accountPath
        ).asyncExecute()

        return AssetBalance(
            accountInfo: responses.first?.value,
            chainAssetId: chainAssetId,
            accountId: accountId
        )
    }
}
