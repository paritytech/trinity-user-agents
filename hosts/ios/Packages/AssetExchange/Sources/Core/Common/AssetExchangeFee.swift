import Foundation
import SubstrateSdk
import ExtrinsicService

public struct AssetExchangeFeeArgs {
    public let route: AssetExchangeRoute
    public let slippage: BigRational
    public let feeAssetId: ChainAssetId

    // if nil then same as signer
    public let destinationAccountId: AccountId?

    public init(
        route: AssetExchangeRoute,
        slippage: BigRational,
        feeAssetId: ChainAssetId,
        destinationAccountId: AccountId?
    ) {
        self.route = route
        self.slippage = slippage
        self.feeAssetId = feeAssetId
        self.destinationAccountId = destinationAccountId
    }
}

public enum AssetExchangeFeeError: Error {
    case mismatchBetweenFeeAndRoute
}

public struct AssetExchangeFee: Equatable {
    public let route: AssetExchangeRoute
    public let operationFees: [AssetExchangeOperationFee]
    public let intermediateFeesInAssetIn: Balance
    public let slippage: BigRational
    public let feeAssetId: ChainAssetId

    public init(
        route: AssetExchangeRoute,
        operationFees: [AssetExchangeOperationFee],
        intermediateFeesInAssetIn: Balance,
        slippage: BigRational,
        feeAssetId: ChainAssetId
    ) {
        self.route = route
        self.operationFees = operationFees
        self.intermediateFeesInAssetIn = intermediateFeesInAssetIn
        self.slippage = slippage
        self.feeAssetId = feeAssetId
    }
}

public extension AssetExchangeFee {
    func originPostsubmissionFeeInAsset(
        _ asset: ChainAssetProtocol,
        matchingPayer: AssetExchangeFeePayerMatcher = .selectedAccount
    ) -> Balance {
        guard let originFee = operationFees.first else {
            return 0
        }

        return originFee.postSubmissionFee.totalAmountIn(
            asset: asset.chainAssetId,
            matchingPayer: matchingPayer
        )
    }

    func originFeeInAsset(
        _ asset: ChainAssetProtocol,
        matchingPayer: AssetExchangeFeePayerMatcher = .selectedAccount
    ) -> Balance {
        guard let originFee = operationFees.first else {
            return 0
        }

        return originFee.totalAmountIn(asset: asset.chainAssetId, matchingPayer: matchingPayer)
    }

    func originExtrinsicFee() -> ExtrinsicFeeProtocol? {
        operationFees.first?.submissionFee
    }

    func postSubmissionFeeInAssetIn(
        _ assetIn: ChainAssetProtocol,
        matchingPayer: AssetExchangeFeePayerMatcher = .selectedAccount
    ) -> Balance {
        guard let originFee = operationFees.first else {
            return 0
        }

        let originFeeAmount = originFee.postSubmissionFee.totalAmountIn(
            asset: assetIn.chainAssetId,
            matchingPayer: matchingPayer
        )

        return originFeeAmount + intermediateFeesInAssetIn
    }

    func totalFeeInAssetIn(
        _ assetIn: ChainAssetProtocol,
        matchingPayer: AssetExchangeFeePayerMatcher = .selectedAccount
    ) -> Balance {
        guard let originFee = operationFees.first else {
            return 0
        }

        let originFeeAmount = originFee.totalAmountIn(
            asset: assetIn.chainAssetId,
            matchingPayer: matchingPayer
        )

        return originFeeAmount + intermediateFeesInAssetIn
    }

    var hasOriginPostSubmissionByAccount: Bool {
        guard let originFee = operationFees.first else {
            return false
        }

        return !originFee.postSubmissionFee.paidByAccount.isEmpty
    }

    func calculateTotalFeeInFiat(
        matching operations: [AssetExchangeMetaOperationProtocol],
        priceStore: AssetExchangePriceStoring
    ) -> Decimal {
        zip(operations, operationFees).map { operation, fee in
            fee.totalInFiat(in: operation.assetIn.chainInterface, priceStore: priceStore)
        }.reduce(Decimal(0)) { $0 + $1 }
    }
}
