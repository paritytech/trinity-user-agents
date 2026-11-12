import Foundation
import TrUAPIHost

final class FundingAmountInteractor: FundingEntryInteractor, FundingAmountInteractorInputProtocol {
    private let environment: FundingFlowEnvironment
    private let balance = FundingFollowing()

    init(service: FundingServiceProtocol, entryId: String, environment: FundingFlowEnvironment) {
        self.environment = environment
        super.init(service: service, entryId: entryId)
    }

    func amountIssue(_ amount: U128, spendable: U128?) -> FundingAmountIssue? {
        service.amountIssue(amount, spendable: spendable, for: entryId)
    }

    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void) {
        balance.task = Task { [environment] in
            await environment.followSpendable(onChange)
        }
    }
}
