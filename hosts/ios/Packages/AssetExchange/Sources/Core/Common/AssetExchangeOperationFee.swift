import Foundation
import BigInt
import SubstrateSdk
import ExtrinsicService

public enum AssetExchangeOperationFeeError: Error {
    case assetMismatch
    case payerMismatch
}

public struct AssetExchangeOperationFee: Equatable {
    public struct Amount: Equatable {
        public let amount: Balance
        public let asset: ChainAssetId

        public init(amount: Balance, asset: ChainAssetId) {
            self.amount = amount
            self.asset = asset
        }

        public func totalAmountEnsuring(asset: ChainAssetId) throws -> Balance {
            guard self.asset == asset else {
                throw AssetExchangeOperationFeeError.assetMismatch
            }

            return amount
        }

        public func totalAmountIn(asset: ChainAssetId) -> Balance {
            self.asset == asset ? amount : 0
        }

        public func addAmount(to store: inout [ChainAssetId: Balance]) {
            store[asset] = (store[asset] ?? 0) + amount
        }
    }

    public struct Submission: Equatable {
        public let amountWithAsset: Amount

        // nil means selected account pays fee
        public let payer: ExtrinsicFeePayer?

        public let weight: Substrate.Weight

        public init(
            amountWithAsset: Amount,
            payer: ExtrinsicFeePayer?,
            weight: Substrate.Weight
        ) {
            self.amountWithAsset = amountWithAsset
            self.payer = payer
            self.weight = weight
        }

        public func totalAmountEnsuring(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) throws -> Balance {
            guard matchingPayer.matches(payer: payer) else {
                throw AssetExchangeOperationFeeError.payerMismatch
            }

            return try amountWithAsset.totalAmountEnsuring(asset: asset)
        }

        public func totalAmountIn(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) -> Balance {
            guard matchingPayer.matches(payer: payer) else {
                return 0
            }

            return amountWithAsset.totalAmountIn(asset: asset)
        }

        public func addAmount(to store: inout [ChainAssetId: Balance]) {
            amountWithAsset.addAmount(to: &store)
        }
    }

    public struct AmountByPayer: Equatable {
        public let amountWithAsset: Amount

        public let payer: ExtrinsicFeePayer?

        public init(
            amountWithAsset: Amount,
            payer: ExtrinsicFeePayer?
        ) {
            self.amountWithAsset = amountWithAsset
            self.payer = payer
        }

        public func totalAmountEnsuring(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) throws -> Balance {
            guard matchingPayer.matches(payer: payer) else {
                throw AssetExchangeOperationFeeError.payerMismatch
            }

            return try amountWithAsset.totalAmountEnsuring(asset: asset)
        }

        public func totalAmountIn(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) -> Balance {
            guard matchingPayer.matches(payer: payer) else {
                return 0
            }

            return amountWithAsset.totalAmountIn(asset: asset)
        }

        public func addAmount(to store: inout [ChainAssetId: Balance]) {
            amountWithAsset.addAmount(to: &store)
        }
    }

    public struct PostSubmission: Equatable {
        /// Post-submission fees paid by (some) origin account.
        /// This is typed as `AmountByAccount` as those fee might still
        /// use different accounts (e.g. delivery fees are always paid from requested account)
        public let paidByAccount: [AmountByPayer]

        /// Post-submission fees paid from swapping amount directly. Its payment is isolated
        /// and does not involve any withdrawals from accounts
        public let paidFromAmount: [Amount]

        public init(
            paidByAccount: [AmountByPayer],
            paidFromAmount: [Amount]
        ) {
            self.paidByAccount = paidByAccount
            self.paidFromAmount = paidFromAmount
        }

        public func totalByAccountEnsuring(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) throws -> Balance {
            try paidByAccount.reduce(0) { total, item in
                let current = try item.totalAmountEnsuring(
                    asset: asset,
                    matchingPayer: matchingPayer
                )

                return total + current
            }
        }

        public func totalByAccountAmountIn(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) -> Balance {
            paidByAccount.reduce(0) { total, item in
                total + item.totalAmountIn(asset: asset, matchingPayer: matchingPayer)
            }
        }

        public func totalFromAmountEnsuring(asset: ChainAssetId) throws -> Balance {
            try paidFromAmount.reduce(0) { total, item in
                let current = try item.totalAmountEnsuring(asset: asset)

                return total + current
            }
        }

        public func totalFromAmountIn(asset: ChainAssetId) -> Balance {
            paidFromAmount.reduce(0) { total, item in
                total + item.totalAmountIn(asset: asset)
            }
        }

        public func totalAmountEnsuring(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) throws -> Balance {
            let totalByAccount = try totalByAccountEnsuring(
                asset: asset,
                matchingPayer: matchingPayer
            )

            let totalFromAmount = try totalFromAmountEnsuring(asset: asset)

            return totalByAccount + totalFromAmount
        }

        public func totalAmountIn(
            asset: ChainAssetId,
            matchingPayer: AssetExchangeFeePayerMatcher
        ) -> Balance {
            let totalByAccount = totalByAccountAmountIn(
                asset: asset,
                matchingPayer: matchingPayer
            )

            let totalFromAmount = totalFromAmountIn(asset: asset)

            return totalByAccount + totalFromAmount
        }

        public func addAmount(to store: inout [ChainAssetId: Balance]) {
            paidByAccount.forEach { $0.amountWithAsset.addAmount(to: &store) }
            paidFromAmount.forEach { $0.addAmount(to: &store) }
        }
    }

    public enum PostTransfer: Equatable {
        // no additional transactions or
        // operation's account the same as destination
        case free

        // there is a separate fee to pay
        case fee(Submission)

        // post transfer is not supported and should be
        // taken care separately
        case notSupported
    }

    ///  Fee that is paid when submitting transaction
    public let submissionFee: Submission

    ///  Fee that is paid after transaction started execution on-chain. For example, delivery fee for the crosschain
    public let postSubmissionFee: PostSubmission

    ///  Fee to pay to credit swapped tokens to a concrete account id
    public let postTransfer: PostTransfer

    public init(
        submissionFee: Submission,
        postSubmissionFee: PostSubmission,
        postTransfer: PostTransfer
    ) {
        self.submissionFee = submissionFee
        self.postSubmissionFee = postSubmissionFee
        self.postTransfer = postTransfer
    }
}

public extension AssetExchangeOperationFee {
    func totalAmountToPayFromSelectedAccount() throws -> Balance {
        let asset = submissionFee.amountWithAsset.asset

        let submissionByAccount = try submissionFee.totalAmountEnsuring(
            asset: asset,
            matchingPayer: .selectedAccount
        )

        let postSubmissionByAccount = try postSubmissionFee.totalByAccountEnsuring(
            asset: asset,
            matchingPayer: .selectedAccount
        )

        return submissionByAccount + postSubmissionByAccount
    }

    func totalToPayFromAmountEnsuring(asset: ChainAssetId) throws -> Balance {
        try postSubmissionFee.totalFromAmountEnsuring(asset: asset)
    }

    func totalEnsuringSubmissionAsset(payerMatcher: AssetExchangeFeePayerMatcher) throws -> Balance {
        let asset = submissionFee.amountWithAsset.asset

        let submissionTotal = try submissionFee.totalAmountEnsuring(
            asset: asset,
            matchingPayer: payerMatcher
        )

        let postSubmissionTotal = try postSubmissionFee.totalAmountEnsuring(
            asset: asset,
            matchingPayer: payerMatcher
        )

        return submissionTotal + postSubmissionTotal
    }

    func totalAmountIn(
        asset: ChainAssetId,
        matchingPayer: AssetExchangeFeePayerMatcher
    ) -> Balance {
        let submissionTotal = submissionFee.totalAmountIn(
            asset: asset,
            matchingPayer: matchingPayer
        )

        let postSubmissionTotal = postSubmissionFee.totalAmountIn(
            asset: asset,
            matchingPayer: matchingPayer
        )

        return submissionTotal + postSubmissionTotal
    }

    func groupedAmountByAsset() -> [ChainAssetId: Balance] {
        var store: [ChainAssetId: Balance] = [:]

        submissionFee.addAmount(to: &store)
        postSubmissionFee.addAmount(to: &store)

        return store
    }

    func totalInFiat(
        in chain: ChainProtocol,
        priceStore: AssetExchangePriceStoring
    ) -> Decimal {
        let amounts = groupedAmountByAsset()

        return amounts
            .map { keyValue in
                guard
                    keyValue.key.chainId == chain.chainId,
                    let assetPrecision = chain.assetInteface(
                        for: keyValue.key.assetId
                    )?.decimalPrecision else {
                    return 0
                }

                return Decimal.fiatValue(
                    from: keyValue.value,
                    price: priceStore.fetchPrice(for: keyValue.key),
                    precision: Int16(assetPrecision)
                )
            }
            .reduce(Decimal(0)) { $1 + $0 }
    }
}

extension AssetExchangeOperationFee.Submission: ExtrinsicFeeProtocol {
    public var amount: Balance { amountWithAsset.amount }
}
