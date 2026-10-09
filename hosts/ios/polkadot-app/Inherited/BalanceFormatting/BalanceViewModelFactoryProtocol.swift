import Foundation
import Foundation_iOS
import SubstrateSdk

protocol BalanceViewModelFactoryProtocol: PrimitiveBalanceViewModelFactoryProtocol {
    func createBalanceInputViewModel(_ amount: Decimal?) -> LocalizableResource<AmountInputViewModelProtocol>
}
