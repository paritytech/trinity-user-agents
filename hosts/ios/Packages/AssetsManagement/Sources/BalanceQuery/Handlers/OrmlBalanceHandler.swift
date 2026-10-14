import ChainStore
import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency
import Operation_iOS

struct OrmlBalanceHandler {
    let chainResource: ChainResourceProtocol
    let operationQueue: OperationQueue

    func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        currencyIdScale: String
    ) async throws -> AssetBalance {
        let connection = try chainResource.getRpcConnectionOrError(
            for: chainAssetId.chainId
        )
        let runtimeProvider = try chainResource.getRuntimeCodingServiceOrError(
            for: chainAssetId.chainId
        )

        let currencyId = try Data(hexString: currencyIdScale)

        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let encodingOperation = DoubleMapKeyEncodingOperation<BytesCodable, BytesCodable>(
            path: OrmlPallet.ormlTokenAccount,
            storageKeyFactory: StorageKeyFactory(),
            keyParams1: [BytesCodable(wrappedValue: accountId)],
            keyParams2: [BytesCodable(wrappedValue: currencyId)],
            param1Encoder: nil,
            param2Encoder: { $0.wrappedValue }
        )

        encodingOperation.codingFactory = codingFactory
        let keys = try await encodingOperation.asyncExecute()

        let requestFactory = StorageRequestFactory.asyncInit()

        let responses: [StorageResponse<OrmlAccount>] = try await requestFactory.queryItems(
            engine: connection,
            keys: { keys },
            factory: { codingFactory },
            storagePath: OrmlPallet.ormlTokenAccount
        ).asyncExecute()

        return AssetBalance(
            ormlAccount: responses.first?.value,
            chainAssetId: chainAssetId,
            accountId: accountId
        )
    }
}
