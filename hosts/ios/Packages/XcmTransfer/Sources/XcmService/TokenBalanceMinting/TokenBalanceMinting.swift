import Foundation
import SubstrateSdk

protocol TokenBalanceMinting {
    func getMintCall(
        for accountId: AccountId,
        amount: Balance
    ) -> RuntimeCallCollecting
}
