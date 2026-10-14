import Foundation
import BigInt
import Foundation_iOS
import ExtrinsicService
import SubstrateSdk
import ChainRegistry

enum TransferDataValidator {
    struct WillTokenBeLost {
        let amount: Decimal?
        let fee: ExtrinsicFeeProtocol?
        let totalAmount: BigUInt?
        let minBalance: BigUInt?
    }
}

protocol TransferDataValidatorFactoryProtocol: BaseDataValidatingFactoryProtocol {
    func willTokenBeLost(
        params: TransferDataValidator.WillTokenBeLost,
        onSendAll: @escaping () -> Void,
        locale: Locale
    ) -> DataValidating

    func receiverWillHaveAssetAccount(
        sendingAmount: Decimal?,
        totalAmount: BigUInt?,
        minBalance: BigUInt?,
        locale: Locale
    ) -> DataValidating

    func receiverDiffers(
        recepient: AccountId?,
        sender: AccountId,
        locale: Locale
    ) -> DataValidating
}

class TransferDataValidatorFactory: TransferDataValidatorFactoryProtocol {
    weak var view: ControllerValidationResultPresentable?

    var basePresentable: ValidationErrorPresentable { presentable }

    let presentable: TransferValidationErrorPresentable
    let chainAsset: ChainAsset

    init(presentable: TransferValidationErrorPresentable, chainAsset: ChainAsset) {
        self.presentable = presentable
        self.chainAsset = chainAsset
    }

    func willTokenBeLost(
        params: TransferDataValidator.WillTokenBeLost,
        onSendAll _: @escaping () -> Void,
        locale: Locale
    ) -> DataValidating {
        let precision = chainAsset.assetDisplayInfo.assetPrecision
        let sendingAmount = params.amount.flatMap {
            $0.toSubstrateAmount(precision: precision)
        }

        let totalAmount = params.totalAmount
        let minBalance = params.minBalance
        let fee = params.fee

        return WarningConditionViolation(onWarning: { [weak self] delegate in
            guard let self, let view else {
                return
            }

            let tokenFormatter = AssetBalanceFormatterFactory()
                .createTokenFormatter(for: chainAsset.assetDisplayInfo)
                .value(for: locale)

            let sendingAndFee = (sendingAmount ?? 0) + (fee?.amountForCurrentAccount ?? 0)
            let amounLost = (totalAmount ?? 0).subtractOrZero(sendingAndFee)

            let amountLostString = tokenFormatter.stringFromDecimal(
                amounLost.decimal(precision: chainAsset.asset.precision)
            )

            let minBalanceString = tokenFormatter.stringFromDecimal(
                minBalance.decimalOrZero(precision: chainAsset.asset.precision)
            )
            basePresentable.presentExistentialDepositWarning(
                from: view,
                amountLost: amountLostString ?? "",
                minBalance: minBalanceString ?? "",
                action: {
                    delegate.didCompleteWarningHandling()
                },
                locale: locale
            )
        }, preservesCondition: {
            guard
                let sendingAmount,
                let totalAmount,
                let minBalance
            else {
                return false
            }

            let feeAmount = fee?.amountForCurrentAccount ?? 0
            return totalAmount >= minBalance + sendingAmount + feeAmount ||
                totalAmount == sendingAmount + feeAmount
        })
    }

    func receiverWillHaveAssetAccount(
        sendingAmount: Decimal?,
        totalAmount: BigUInt?,
        minBalance: BigUInt?,
        locale: Locale
    ) -> DataValidating {
        let sendingAmountValue: BigUInt

        if let sendingAmount {
            let precision = chainAsset.assetDisplayInfo.assetPrecision
            sendingAmountValue = sendingAmount.toSubstrateAmount(precision: precision) ?? 0
        } else {
            sendingAmountValue = 0
        }

        return ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentReceiverBalanceTooLow(from: view, locale: locale)
        }, preservesCondition: {
            guard let minBalance else {
                return false
            }

            guard let totalAmount else {
                return sendingAmountValue >= minBalance
            }

            return totalAmount + sendingAmountValue >= minBalance
        })
    }

    func receiverDiffers(
        recepient: AccountId?,
        sender: AccountId,
        locale: Locale
    ) -> DataValidating {
        ErrorConditionViolation(onError: { [weak self] in
            guard let view = self?.view else {
                return
            }

            self?.presentable.presentSameReceiver(from: view, locale: locale)
        }, preservesCondition: {
            recepient != sender
        })
    }
}
