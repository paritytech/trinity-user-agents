import Foundation
import Foundation_iOS
import BigInt
import ExtrinsicService
import SubstrateSdk

@MainActor
protocol BaseDataValidatingFactoryProtocol: AnyObject {
    var view: ControllerValidationResultPresentable? { get }
    var basePresentable: ValidationErrorPresentable { get }

    func hasNonZero(balance: Decimal?, locale: Locale) -> DataValidating
    func hasAmount(balance: Decimal?, locale: Locale) -> DataValidating
    func canSpendAmount(
        balance: Decimal?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale,
        onError: (() -> Void)?
    ) -> DataValidating
    func feeIsCalculated(fee: ExtrinsicFeeProtocol?, locale: Locale) -> DataValidating
    func canPayFeeSpendingAmount(
        balance: Decimal?,
        fee: ExtrinsicFeeProtocol?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating
    func has(
        fee: ExtrinsicFeeProtocol?,
        locale: Locale,
        onError: (() -> Void)?
    ) -> DataValidating
    func accountIsNotSystem(
        for accountId: AccountId?,
        locale: Locale
    ) -> DataValidating
    func notViolatingMinBalancePaying(
        fee: ExtrinsicFeeProtocol?,
        total: BigUInt?,
        minBalance: BigUInt?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating
}

extension BaseDataValidatingFactoryProtocol {
    func hasNonZero(balance: Decimal?, locale _: Locale) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }
            let message = String(localized: .Validation.insufficientBalance)

            self?.basePresentable.presentInsufficientBalance(with: message, on: view)
        }, preservesCondition: {
            guard let balance, balance > 0 else {
                return false
            }

            return true
        })
    }

    func hasAmount(balance: Decimal?, locale: Locale) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.basePresentable.presentEnterAmount(from: view, locale: locale)
        }, preservesCondition: {
            guard let balance, balance > 0 else {
                return false
            }

            return true
        })
    }

    func canSpendAmount(
        balance: Decimal?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale,
        onError: (() -> Void)? = nil
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            defer {
                onError?()
            }
            guard let view = self?.view else {
                return
            }

            let tokenFormatter = AssetBalanceFormatterFactory().createTokenFormatter(for: asset)
            let balanceString = tokenFormatter.value(for: locale).stringFromDecimal(balance ?? 0) ?? ""

            self?.basePresentable.presentAmountTooHigh(from: view, maxBalance: balanceString, locale: locale)
        }, preservesCondition: {
            if let balance, let amount = spendingAmount {
                amount <= balance
            } else {
                false
            }
        })
    }

    func canPayFee(
        balance: Decimal?,
        fee: ExtrinsicFeeProtocol?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            let tokenFormatter = AssetBalanceFormatterFactory().createTokenFormatter(for: asset)

            let balanceString = tokenFormatter.value(for: locale).stringFromDecimal(balance ?? 0) ?? ""
            let feeDecimal = fee?.amountForCurrentAccount?.decimal(assetInfo: asset)
            let feeString = tokenFormatter.value(for: locale).stringFromDecimal(feeDecimal ?? 0) ?? ""

            self?.basePresentable.presentFeeTooHigh(from: view, balance: balanceString, fee: feeString, locale: locale)
        }, preservesCondition: {
            guard let balance, let fee else {
                return false
            }

            guard let feeAmountInPlank = fee.amountForCurrentAccount else {
                return true
            }

            let feeAmount = feeAmountInPlank.decimal(assetInfo: asset)

            return feeAmount <= balance
        })
    }

    func canPayFeeSpendingAmount(
        balance: Decimal?,
        fee: ExtrinsicFeeProtocol?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating {
        let targetAmount = spendingAmount ?? 0

        if let balance {
            let targetBalance = balance >= targetAmount ? balance - targetAmount : 0
            return canPayFee(
                balance: targetBalance,
                fee: fee,
                asset: asset,
                locale: locale
            )
        } else {
            return canPayFee(balance: nil, fee: fee, asset: asset, locale: locale)
        }
    }

    func has(fee: ExtrinsicFeeProtocol?, locale: Locale, onError: (() -> Void)?) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            defer {
                onError?()
            }

            guard let view = self?.view else {
                return
            }

            self?.basePresentable.presentFeeNotReceived(from: view, locale: locale)
        }, preservesCondition: { fee != nil })
    }

    func feeIsCalculated(fee: ExtrinsicFeeProtocol?, locale: Locale) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.basePresentable.presentFeeNotCalculatedYet(from: view, locale: locale)
        }, preservesCondition: {
            fee?.amountForCurrentAccount != nil
        })
    }

    func canSpendAmountInPlank(
        balance: BigUInt?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale,
        onError: (() -> Void)? = nil
    ) -> DataValidating {
        let precision = asset.assetPrecision
        let balanceDecimal = balance.flatMap { Decimal.fromSubstrateAmount($0, precision: precision) }

        return canSpendAmount(
            balance: balanceDecimal,
            spendingAmount: spendingAmount,
            asset: asset,
            locale: locale,
            onError: onError
        )
    }

    func canPayFeeInPlank(
        balance: BigUInt?,
        fee: ExtrinsicFeeProtocol?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating {
        let precision = asset.assetPrecision
        let balanceDecimal = balance.flatMap { Decimal.fromSubstrateAmount($0, precision: precision) }

        return canPayFee(
            balance: balanceDecimal,
            fee: fee,
            asset: asset,
            locale: locale
        )
    }

    func canPayFeeSpendingAmountInPlank(
        balance: BigUInt?,
        fee: ExtrinsicFeeProtocol?,
        spendingAmount: Decimal?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating {
        let precision = asset.assetPrecision
        let balanceDecimal = balance.flatMap { Decimal.fromSubstrateAmount($0, precision: precision) }

        return canPayFeeSpendingAmount(
            balance: balanceDecimal,
            fee: fee,
            spendingAmount: spendingAmount,
            asset: asset,
            locale: locale
        )
    }

    func accountIsNotSystem(for accountId: AccountId?, locale: Locale) -> DataValidating {
        WarningConditionViolation(onWarning: { [weak self] delegate in
            guard let view = self?.view else {
                return
            }

            self?.basePresentable.presentIsSystemAccount(
                from: view,
                onContinue: {
                    delegate.didCompleteWarningHandling()
                },
                locale: locale
            )
        }, preservesCondition: {
            guard let accountId else {
                return true
            }

            let validation = CompoundSystemAccountValidation.substrateAccounts()

            return !validation.isSystem(accountId: accountId)
        })
    }

    func notViolatingMinBalancePaying(
        fee: ExtrinsicFeeProtocol?,
        total: BigUInt?,
        minBalance: BigUInt?,
        asset: AssetBalanceDisplayInfo,
        locale: Locale
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            let tokenFormatter = AssetBalanceFormatterFactory()
                .createTokenFormatter(for: asset)
                .value(for: locale)

            let feeDecimal = fee?.amountForCurrentAccount?.decimal(assetInfo: asset) ?? 0
            let minBalanceDecimal = minBalance?.decimal(assetInfo: asset) ?? 0
            let feeAndMinBalanceDecimal = feeDecimal + minBalanceDecimal
            let totalDecimal = total?.decimal(assetInfo: asset) ?? 0
            let needToAddDecimal = max(feeAndMinBalanceDecimal - totalDecimal, 0)

            let totalString = tokenFormatter.stringFromDecimal(totalDecimal)
            let feeAndMinBalanceString = tokenFormatter.stringFromDecimal(feeAndMinBalanceDecimal)
            let needToAddString = tokenFormatter.stringFromDecimal(needToAddDecimal)

            self?.basePresentable.presentMinBalanceViolated(
                from: view,
                minBalanceForOperation: feeAndMinBalanceString ?? "",
                currentBalance: totalString ?? "",
                needToAddBalance: needToAddString ?? "",
                locale: locale
            )
        }, preservesCondition: {
            guard let feeAmount = fee?.amountForCurrentAccount else {
                return true
            }

            if let total, let minBalance {
                return feeAmount + minBalance <= total
            } else {
                return false
            }
        })
    }
}
