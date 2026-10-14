import Foundation
import SubstrateSdk

public struct TokenDepositEvent {
    public let accountId: AccountId
    public let amount: Balance

    public init(accountId: AccountId, amount: Balance) {
        self.accountId = accountId
        self.amount = amount
    }
}

public protocol TokenDepositEventMatching {
    func matchDeposit(
        event: Event,
        using codingFactory: RuntimeCoderFactoryProtocol
    ) -> TokenDepositEvent?
}
