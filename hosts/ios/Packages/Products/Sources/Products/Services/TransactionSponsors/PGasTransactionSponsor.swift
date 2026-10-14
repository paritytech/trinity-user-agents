import AssetsManagement
import BigInt
import ChainStore
import Foundation
import Individuality
import Operation_iOS
import Revive
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency
import SubstrateOperation
import SDKLogger

public protocol PGasTransactionSponsoring {
    func sponsorIfNeeded(
        productAccount: ProductAccountId,
        callData: Data,
        chainId: ChainId
    ) async throws
}

public enum PGasTransactionSponsorError: Error {
    case pgasAssetUnavailable(ChainAssetId)
}

public final class PGasTransactionSponsor {
    private static let sufficientBalancePercent: BigUInt = 20

    private let pgasChainAssetId: ChainAssetId
    private let assetQueryTypeMaker: AssetQueryTypeMaking
    private let balanceService: BalanceQueryServicing
    private let accountManager: ProductsAccountManaging
    private let chainResource: ChainResourceProtocol
    private let operationQueue: OperationQueue
    private let logger: SDKLoggerProtocol

    public init(
        pgasChainAssetId: ChainAssetId,
        assetQueryTypeMaker: AssetQueryTypeMaking,
        balanceService: BalanceQueryServicing,
        accountManager: ProductsAccountManaging,
        chainResource: ChainResourceProtocol,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.pgasChainAssetId = pgasChainAssetId
        self.assetQueryTypeMaker = assetQueryTypeMaker
        self.balanceService = balanceService
        self.accountManager = accountManager
        self.chainResource = chainResource
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension PGasTransactionSponsor: PGasTransactionSponsoring {
    public func sponsorIfNeeded(
        productAccount: ProductAccountId,
        callData: Data,
        chainId: ChainId
    ) async throws {
        guard chainId == pgasChainAssetId.chainId else { return }

        let runtimeCodingService = try chainResource.getRuntimeCodingServiceOrError(for: chainId)

        let hasReviveCall = try? await NestedCallMapper().hasMachingCall(
            in: callData,
            runtimeCodingService: runtimeCodingService
        ) { call, _ in
            call.moduleName == RevivePallet.name
        }

        guard hasReviveCall == true else { return }

        logger.debug("Revive call found. Checking PGas balance...")

        let chain = try chainResource.getChainInterfaceOrError(for: chainId)

        guard
            let chainAsset = chain.chainAssetInterface(for: pgasChainAssetId.assetId),
            let queryType = assetQueryTypeMaker.deriveQueryType(chainAsset)
        else {
            throw PGasTransactionSponsorError.pgasAssetUnavailable(pgasChainAssetId)
        }

        let accountId = try accountManager.deriveAccount(productAccount)

        let balance = try await balanceService.queryBalance(
            for: accountId,
            chainAssetId: pgasChainAssetId,
            assetParams: queryType
        )

        let claimAmount = try await fetchPgasClaimAmount()
        let threshold = BigRational.percent(of: Self.sufficientBalancePercent).mul(value: claimAmount)

        guard balance.transferable < threshold else { return }

        logger.debug("Allocating PGas")

        _ = try await accountManager.requestResourceAllocation(
            for: productAccount.productId,
            resources: [.smartContractAllowance(dest: productAccount.derivationIndex)],
            policy: .increase
        )
    }
}

// MARK: - Private

private extension PGasTransactionSponsor {
    func fetchPgasClaimAmount() async throws -> BigUInt {
        let runtimeProvider = try chainResource.getRuntimeCodingServiceOrError(
            for: pgasChainAssetId.chainId
        )

        let claimAmount: BigUInt = try await PrimitiveConstantOperation.wrapper(
            for: PGASPallet.Constants.pgasClaimAmount(),
            runtimeService: runtimeProvider
        )
        .asyncExecute()

        return claimAmount
    }
}
