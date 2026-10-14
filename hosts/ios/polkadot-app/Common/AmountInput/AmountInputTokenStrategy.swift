import Foundation
import Foundation_iOS
import BigInt

final class AmountInputTokenStrategy {
    let chainAsset: AssetBalanceDisplayInfo
    let balanceViewModelFactory: BalanceViewModelFactoryProtocol

    init(
        chainAsset: AssetBalanceDisplayInfo,
        balanceViewModelFactory: BalanceViewModelFactoryProtocol
    ) {
        self.chainAsset = chainAsset
        self.balanceViewModelFactory = balanceViewModelFactory
    }
}

extension AmountInputTokenStrategy: AmountInputStrategyProtocol {
    func createInputViewModelFactory(
        for inputAmount: AmountInputResult?,
        balance: BigUInt?
    ) -> AmountInputViewModelProtocol {
        guard
            let inputAmount,
            let decimalBalance = balance?.decimal(assetInfo: chainAsset)
        else {
            return balanceViewModelFactory.createBalanceInputViewModel(nil)
                .value(for: .current)
        }

        let decimalAmount =
            switch inputAmount {
            case let .rate(value):
                max(value * decimalBalance, 0.0)
            case let .absolute(value):
                value
            }

        return balanceViewModelFactory.createBalanceInputViewModel(
            decimalAmount
        )
        .value(for: .current)
    }

    func createAssetViewModel() -> AssetAmountViewModel {
        AssetAmountViewModel(
            symbol: chainAsset.symbol,
            isSymbolInFront: chainAsset.symbolPosition == .prefix
        )
    }
}
