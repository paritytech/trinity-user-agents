import Foundation
import ExtrinsicService

public extension AssetExchangeOperationFee {
    init(
        extrinsicFee: ExtrinsicFeeProtocol,
        args: AssetExchangeAtomicOperationArgs,
        postTransfer: PostTransfer
    ) {
        submissionFee = .init(
            amountWithAsset: .init(
                amount: extrinsicFee.amount,
                asset: args.feeAsset
            ),
            payer: extrinsicFee.payer,
            weight: extrinsicFee.weight
        )

        postSubmissionFee = .init(paidByAccount: [], paidFromAmount: [])

        self.postTransfer = postTransfer
    }
}
