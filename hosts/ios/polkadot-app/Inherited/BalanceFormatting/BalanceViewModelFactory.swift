import Foundation
import Foundation_iOS
import BigInt
import SubstrateSdk

final class BalanceViewModelFactory: PrimitiveBalanceViewModelFactory {
    let limit: Decimal

    init(
        targetAssetInfo: AssetBalanceDisplayInfo,
        formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory(),
        limit: Decimal = Decimal.greatestFiniteMagnitude
    ) {
        self.limit = limit

        super.init(
            targetAssetInfo: targetAssetInfo,
            formatterFactory: formatterFactory
        )
    }
}

extension BalanceViewModelFactory: BalanceViewModelFactoryProtocol {
    func createBalanceInputViewModel(
        _ amount: Decimal?
    ) -> LocalizableResource<AmountInputViewModelProtocol> {
        let localizableFormatter = formatterFactory.createInputFormatter(for: targetAssetInfo)
        let symbol = targetAssetInfo.symbol

        let currentLimit = limit

        return LocalizableResource { locale in
            let formatter = localizableFormatter.value(for: locale)
            return AmountInputViewModel(
                symbol: symbol,
                amount: amount,
                limit: currentLimit,
                formatter: formatter,
                precision: Int16(formatter.maximumFractionDigits)
            )
        }
    }
}
