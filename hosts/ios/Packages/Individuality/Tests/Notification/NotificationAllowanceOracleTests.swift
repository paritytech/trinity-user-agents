import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
import Testing
@testable import Individuality

struct NotificationAllowanceOracleTests {
    private let repository = FakeStatementStoreAllowanceRepository()

    @Test func answersPresentAbsentAndLeavesUnansweredTargetsUndecided() async throws {
        let granted = claim(for: .target(1))
        let missing = claim(for: .target(2))
        let unanswered = claim(for: .target(3))
        let foreign = DurableTxEntry.fixture(domainId: NotificationSlotDomain.domainId, groupId: "other")
        repository.allowances = .success([.target(1): true, .target(2): false])

        let scope = try await openPass(over: [granted, missing, unanswered, foreign])

        #expect(scope.provenCompleted(granted, at: .best))
        #expect(scope.provenNotCompleted(missing, at: .best))
        #expect(!scope.provenCompleted(unanswered, at: .best) && !scope.provenNotCompleted(unanswered, at: .best))
        #expect(!scope.provenCompleted(foreign, at: .best) && !scope.provenNotCompleted(foreign, at: .best))
    }

    @Test func failedReadDecidesNothing() async throws {
        let pending = claim(for: .target(1))
        repository.allowances = .failure(TestReadError())

        let scope = try await openPass(over: [pending])

        #expect(!scope.provenCompleted(pending, at: .finalized) && !scope.provenNotCompleted(pending, at: .finalized))
    }

    private func openPass(over transactions: [DurableTxEntry]) async throws -> any TxCompletionPassScope {
        let oracle = NotificationAllowanceOracle.make(
            chainId: "people",
            allowanceRepository: repository,
            logger: FakeLogger()
        )

        return try await oracle.openPass(
            transactions: transactions,
            ledger: SnapshotLedgerView(transactions: transactions),
            view: StubPinnedChainView()
        )
    }

    private func claim(for target: AccountId) -> DurableTxEntry {
        .fixture(domainId: NotificationSlotDomain.domainId, groupId: NotificationSlotDomain.groupId(for: target))
    }
}

private struct TestReadError: Error {}
