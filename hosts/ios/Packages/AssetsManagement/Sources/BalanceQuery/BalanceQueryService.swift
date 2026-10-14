import ChainStore
import Foundation
import SubstrateSdk

public protocol BalanceQueryServicing {
    func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        assetParams: AssetQueryType
    ) async throws -> AssetBalance
}

public final class BalanceQueryService {
    private let chainResource: ChainResourceProtocol
    private let operationQueue: OperationQueue

    public init(
        chainResource: ChainResourceProtocol,
        operationQueue: OperationQueue
    ) {
        self.chainResource = chainResource
        self.operationQueue = operationQueue
    }
}

extension BalanceQueryService: BalanceQueryServicing {
    public func queryBalance(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        assetParams: AssetQueryType
    ) async throws -> AssetBalance {
        switch assetParams {
        case .native:
            try await NativeBalanceHandler(
                chainResource: chainResource,
                operationQueue: operationQueue
            ).queryBalance(
                for: accountId,
                chainAssetId: chainAssetId
            )

        case let .statemine(assetId, palletName):
            try await AssetsBalanceHandler(
                chainResource: chainResource,
                operationQueue: operationQueue
            ).queryBalance(
                for: accountId,
                chainAssetId: chainAssetId,
                assetId: assetId,
                palletName: palletName
            )

        case let .orml(currencyIdScale):
            try await OrmlBalanceHandler(
                chainResource: chainResource,
                operationQueue: operationQueue
            ).queryBalance(
                for: accountId,
                chainAssetId: chainAssetId,
                currencyIdScale: currencyIdScale
            )

        case let .hydrationEvm(remoteAssetId):
            try await HydrationEvmBalanceHandler(
                chainResource: chainResource,
                operationQueue: operationQueue
            ).queryBalance(
                for: accountId,
                chainAssetId: chainAssetId,
                remoteAssetId: remoteAssetId
            )
        }
    }
}
