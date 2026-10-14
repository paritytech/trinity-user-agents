import BigInt
import Foundation
import Foundation_iOS
import SubstrateSdk
import FoundationExt

class PrimitiveBalanceViewModelFactory {
    let targetAssetInfo: AssetBalanceDisplayInfo
    let priceDisplayInfo: AssetBalanceDisplayInfo
    let formatterFactory: AssetBalanceFormatterFactoryProtocol

    init(
        targetAssetInfo: AssetBalanceDisplayInfo,
        formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory(),
        priceDisplayInfo: AssetBalanceDisplayInfo = .usd
    ) {
        self.targetAssetInfo = targetAssetInfo
        self.formatterFactory = formatterFactory
        self.priceDisplayInfo = priceDisplayInfo
    }
}

extension PrimitiveBalanceViewModelFactory: PrimitiveBalanceViewModelFactoryProtocol {
    func amountFromValue(
        _ value: Balance,
        roundingMode _: NumberFormatter.RoundingMode
    ) -> LocalizableResource<String> {
        let decimalValue = value.decimal(assetInfo: targetAssetInfo)

        let localizableFormatter = formatterFactory.createAssetPriceFormatter(
            for: targetAssetInfo,
            minimumFractionDigits: decimalValue.hasFraction ? targetAssetInfo.displayPrecision : 0
        )

        return LocalizableResource { locale in
            let formatter = localizableFormatter.value(for: locale)
            return formatter.stringFromDecimal(decimalValue) ?? ""
        }
    }

    func balanceFromPrice(
        _ amount: Balance,
        priceData: PriceData?,
        roundingMode _: NumberFormatter.RoundingMode,
        zeroIfNoPrice: Bool
    ) -> LocalizableResource<BalanceViewModelProtocol> {
        let amountDecimal = amount.decimal(assetInfo: targetAssetInfo)
        let localizableAmountFormatter = formatterFactory.createAssetPriceFormatter(
            for: targetAssetInfo,
            minimumFractionDigits: amountDecimal.hasFraction ? targetAssetInfo.displayPrecision : 0
        )

        let localizablePriceFormatter = formatterFactory.createAssetPriceFormatter(
            for: priceDisplayInfo
        )

        return LocalizableResource { locale in
            let amountFormatter = localizableAmountFormatter.value(for: locale)

            let amountString = amountFormatter.stringFromDecimal(amountDecimal) ?? ""

            if !zeroIfNoPrice, priceData == nil {
                return BalanceViewModel(amount: amountString, price: nil)
            }

            let rate = priceData.flatMap { Decimal(string: $0.price) } ?? 0

            let targetAmount = rate * amountDecimal

            let priceFormatter = localizablePriceFormatter.value(for: locale)
            let priceString = priceFormatter.stringFromDecimal(targetAmount) ?? ""

            return BalanceViewModel(amount: amountString, price: priceString)
        }
    }
}
