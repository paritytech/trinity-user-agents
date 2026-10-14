import Foundation
import SubstrateSdk
import ExtrinsicService
import AssetExchange

extension AssetExchangeOperationFee {
    init(
        crosschainFee: XcmFeeModelProtocol,
        originFee: ExtrinsicFeeProtocol,
        assetIn: ChainAssetId,
        assetOut _: ChainAssetId,
        originUtilityAsset: ChainAssetId,
        args: AssetExchangeAtomicOperationArgs
    ) {
        let submissionFee = Submission(
            amountWithAsset: .init(
                amount: originFee.amount,
                asset: args.feeAsset
            ),
            payer: originFee.payer,
            weight: originFee.weight
        )

        let paidByAccount: [AmountByPayer] =
            if crosschainFee.senderPart > 0 {
                [
                    .init(
                        amountWithAsset: .init(amount: crosschainFee.senderPart, asset: originUtilityAsset),
                        payer: nil
                    )
                ]
            } else {
                []
            }

        let paidFromAmount: [Amount] =
            if crosschainFee.holdingPart > 0 {
                [
                    .init(amount: crosschainFee.holdingPart, asset: assetIn)
                ]
            } else {
                []
            }

        let postSubmissionFee = PostSubmission(
            paidByAccount: paidByAccount,
            paidFromAmount: paidFromAmount
        )

        self.init(
            submissionFee: submissionFee,
            postSubmissionFee: postSubmissionFee,
            postTransfer: .free
        )
    }
}
