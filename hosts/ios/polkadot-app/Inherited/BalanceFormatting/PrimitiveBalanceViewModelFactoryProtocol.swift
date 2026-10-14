import BigInt
import Foundation
import Foundation_iOS
import SubstrateSdk

protocol PrimitiveBalanceViewModelFactoryProtocol {
    func amountFromValue(
        _ value: Balance,
        roundingMode: NumberFormatter.RoundingMode
    ) -> LocalizableResource<String>

    func balanceFromPrice(
        _ amount: Balance,
        priceData: PriceData?,
        roundingMode: NumberFormatter.RoundingMode,
        zeroIfNoPrice: Bool
    ) -> LocalizableResource<BalanceViewModelProtocol>

    func balanceFromPrice(
        _ amount: Decimal,
        priceData: PriceData?
    ) -> LocalizableResource<BalanceViewModelProtocol>
}

extension PrimitiveBalanceViewModelFactoryProtocol {
    func balanceFromPrice(_ amount: Balance, priceData: PriceData?) -> LocalizableResource<BalanceViewModelProtocol> {
        balanceFromPrice(amount, priceData: priceData, roundingMode: .down, zeroIfNoPrice: true)
    }

    func amountFromValue(_ value: Balance) -> LocalizableResource<String> {
        amountFromValue(value, roundingMode: .down)
    }
}

extension PrimitiveBalanceViewModelFactoryProtocol where Self: PrimitiveBalanceViewModelFactory {
    func balanceFromPrice(
        _ amount: Decimal,
        priceData: PriceData?
    ) -> LocalizableResource<BalanceViewModelProtocol> {
        let amount = amount.toSubstrateAmount(precision: targetAssetInfo.assetPrecision) ?? 0
        return balanceFromPrice(amount, priceData: priceData)
    }
}
