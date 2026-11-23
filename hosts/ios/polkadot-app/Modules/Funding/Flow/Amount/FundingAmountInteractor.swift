import Foundation
import SubstrateSdk
import TrUAPIHost

final class FundingAmountInteractor: FundingEntryInteractor, FundingAmountInteractorInputProtocol {
    private let environment: FundingFlowEnvironment
    private let balance = FundingFollowing()
    private let entries = FundingFollowing()

    init(service: FundingServiceProtocol, entryId: String, environment: FundingFlowEnvironment) {
        self.environment = environment
        super.init(service: service, entryId: entryId)
    }

    func amountIssue(_ amount: Balance, spendable: Balance?) -> FundingAmountIssue? {
        service.amountIssue(amount, spendable: spendable, for: entryId)
    }

    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void) {
        balance.task = Task { [environment] in
            await environment.followSpendable(onChange)
        }
    }

    func followEntries(_ onChange: @escaping @MainActor ([FundingEntry]) -> Void) {
        entries.task = Task { [service] in
            do {
                for try await entries in service.subscribeEntries() {
                    onChange(entries)
                }
            } catch {}
        }
    }
}
