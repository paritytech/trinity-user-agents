import Foundation
import BigInt

protocol AmountInputStrategyProtocol {
    func createInputViewModelFactory(
        for inputAmount: AmountInputResult?,
        balance: BigUInt?
    ) -> AmountInputViewModelProtocol

    func createAssetViewModel() -> AssetAmountViewModel
}
