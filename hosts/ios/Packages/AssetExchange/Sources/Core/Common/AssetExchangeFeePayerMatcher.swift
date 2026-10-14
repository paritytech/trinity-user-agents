import Foundation
import ExtrinsicService
import SubstrateSdk

public enum AssetExchangeFeePayerMatcher {
    case selectedAccount
    case anyAccount
    case givenAccount(AccountId)

    func matches(payer: ExtrinsicFeePayer?) -> Bool {
        switch self {
        case .selectedAccount:
            payer == nil
        case .anyAccount:
            true
        case let .givenAccount(accountId):
            payer?.accountId == accountId
        }
    }
}
