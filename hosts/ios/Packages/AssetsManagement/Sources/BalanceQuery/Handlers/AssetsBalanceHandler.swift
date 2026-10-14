import ChainStore
import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency
import Operation_iOS

struct AssetsBalanceHandler {
    let chainResource: ChainResourceProtocol
    let operationQueue: OperationQueue

    func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        assetId: String,
        palletName: String?
    ) async throws -> AssetBalance {
        let runtimeProvider = try chainResource.getRuntimeCodingServiceOrError(
            for: chainAssetId.chainId
        )
        let connection = try chainResource.getRpcConnectionOrError(
            for: chainAssetId.chainId
        )

        async let account = queryAssetsAccount(
            for: accountId,
            assetId: assetId,
            palletName: palletName,
            connection: connection,
            runtimeProvider: runtimeProvider
        )

        async let details = queryAssetsDetails(
            assetId: assetId,
            palletName: palletName,
            connection: connection,
            runtimeProvider: runtimeProvider
        )

        return try await AssetBalance(
            assetsAccount: account,
            assetsDetails: details,
            chainAssetId: chainAssetId,
            accountId: accountId
        )
    }
}

// MARK: - Private

private extension AssetsBalanceHandler {
    func queryAssetsAccount(
        for accountId: AccountId,
        assetId: String,
        palletName: String?,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol
    ) async throws -> AssetsPallet.Account? {
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let path = AssetsPallet.accountPath(from: palletName)

        let encodingOperation = DoubleMapKeyEncodingOperation<String, BytesCodable>(
            path: path,
            storageKeyFactory: StorageKeyFactory(),
            keyParams1: [assetId],
            keyParams2: [BytesCodable(wrappedValue: accountId)],
            param1Encoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: assetId),
            param2Encoder: nil
        )

        encodingOperation.codingFactory = codingFactory
        let keys = try await encodingOperation.asyncExecute()

        let requestFactory = StorageRequestFactory.asyncInit()

        let responses: [StorageResponse<AssetsPallet.Account>] = try await requestFactory.queryItems(
            engine: connection,
            keys: { keys },
            factory: { codingFactory },
            storagePath: path
        ).asyncExecute()

        return responses.first?.value
    }

    func queryAssetsDetails(
        assetId: String,
        palletName: String?,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol
    ) async throws -> AssetsPallet.Details? {
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
        let path = AssetsPallet.detailsPath(from: palletName)

        let encodingOperation = MapKeyEncodingOperation<String>(
            path: path,
            storageKeyFactory: StorageKeyFactory(),
            keyParams: [assetId],
            paramEncoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: assetId)
        )

        encodingOperation.codingFactory = codingFactory
        let keys = try await encodingOperation.asyncExecute()

        let requestFactory = StorageRequestFactory.asyncInit()

        let responses: [StorageResponse<AssetsPallet.Details>] = try await requestFactory.queryItems(
            engine: connection,
            keys: { keys },
            factory: { codingFactory },
            storagePath: path
        ).asyncExecute()

        return responses.first?.value
    }
}
