import Foundation
import SubstrateSdk
import TrUAPIHost

final class FundingAmountInteractor: FundingEntryInteractor, FundingAmountInteractorInputProtocol {
    private let environment: FundingFlowEnvironment
    private var balanceTask: Task<Void, Never>?
    private var entriesTask: Task<Void, Never>?

    init(service: FundingServiceProtocol, entryId: String, environment: FundingFlowEnvironment) {
        self.environment = environment
        super.init(service: service, entryId: entryId)
    }

    deinit {
        balanceTask?.cancel()
        entriesTask?.cancel()
    }

    func amountIssue(_ amount: Balance, spendable: Balance?) -> FundingAmountIssue? {
        service.amountIssue(amount, spendable: spendable, for: entryId)
    }

    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void) {
        balanceTask?.cancel()
        balanceTask = Task { [environment] in
            await environment.followSpendable(onChange)
        }
    }

    func followEntries(_ onChange: @escaping @MainActor ([FundingEntry]) -> Void) {
        entriesTask?.cancel()
        entriesTask = Task { [service, logger] in
            do {
                for try await entries in service.subscribeEntries() {
                    onChange(entries)
                }
            } catch {
                if !Task.isCancelled {
                    logger.error("[funding] following the sessions failed: \(error)")
                }
            }
        }
    }
}
