import AssetsManagement
import BigInt
import ChainRegistry
import ChainStore
import Coinage
import Foundation
import Individuality
import Operation_iOS
import SubstrateSdk

enum CoinageDataStorePgasError: Error {
    case pgasAssetUnavailable(ChainAssetId)
}

/// The transferable PGAS balance of the data store account, read the way the product sponsor reads it.
final class CoinageDataStorePgasProvider: PGASBalanceProviding, @unchecked Sendable {
    private let pgasChainAssetId: ChainAssetId
    private let chainResource: ChainResourceProtocol
    private let assetQueryTypeMaker: AssetQueryTypeMaking
    private let balanceService: BalanceQueryServicing

    init(
        pgasChainAssetId: ChainAssetId = AppConfig.Assets.pgasAsset,
        chainResource: ChainResourceProtocol,
        assetQueryTypeMaker: AssetQueryTypeMaking = AssetQueryTypeFactory(),
        balanceService: BalanceQueryServicing
    ) {
        self.pgasChainAssetId = pgasChainAssetId
        self.chainResource = chainResource
        self.assetQueryTypeMaker = assetQueryTypeMaker
        self.balanceService = balanceService
    }

    func transferableBalance(of accountId: AccountId) async throws -> BigUInt {
        let chain = try chainResource.getChainInterfaceOrError(for: pgasChainAssetId.chainId)

        guard
            let chainAsset = chain.chainAssetInterface(for: pgasChainAssetId.assetId),
            let queryType = assetQueryTypeMaker.deriveQueryType(chainAsset)
        else {
            throw CoinageDataStorePgasError.pgasAssetUnavailable(pgasChainAssetId)
        }

        return try await balanceService.queryBalance(
            for: accountId,
            chainAssetId: pgasChainAssetId,
            assetParams: queryType
        ).transferable
    }
}

extension PGASAccountProvisioner {
    /// The data store account's PGAS provisioner over the app's shared PGAS allowance manager.
    static func forDataStoreAccount(
        allowanceManager: AllowanceManaging,
        chainRegistry: ChainRegistryProtocol
    ) -> PGASAccountProvisioner {
        PGASAccountProvisioner(
            allowanceManager: allowanceManager,
            balanceProvider: CoinageDataStorePgasProvider(
                chainResource: chainRegistry,
                balanceService: BalanceQueryService(
                    chainResource: chainRegistry,
                    operationQueue: OperationManagerFacade.sharedDefaultQueue
                )
            ),
            issueDiagnostics: .make(flow: "pgas-allowance", reporter: IssueReportingFacade.shared)
        )
    }
}
