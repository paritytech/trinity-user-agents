import Foundation
import Individuality
import os
import SubstrateSdk

/// Records funding calls and keeps one record period like the allowance manager; `holdNext()` keeps the next
/// call pending until `release()`.
final class FakeStatementAccountFunding: StatementAccountFunding, @unchecked Sendable {
    private struct State {
        var calls: [AccountId] = []
        var priorities: [AllowanceRecord.Priority] = []
        var recordPeriod: UInt32?
        var periodOnFund: UInt32 = 0
        var failuresLeft = 0
        var failure: Error = FakeStatementAccountFundingError()
        var holdsNext = false
        var released = false
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var calls: [AccountId] { state.withLock { $0.calls } }
    var priorities: [AllowanceRecord.Priority] { state.withLock { $0.priorities } }

    func setRecord(period: UInt32?) {
        state.withLock { $0.recordPeriod = period }
    }

    func recordOnFund(period: UInt32) {
        state.withLock { $0.periodOnFund = period }
    }

    func fail(times: Int, with failure: Error = FakeStatementAccountFundingError()) {
        state.withLock { state in
            state.failuresLeft = times
            state.failure = failure
        }
    }

    func holdNext() {
        state.withLock { $0.holdsNext = true }
    }

    func release() {
        state.withLock { $0.released = true }
    }

    func hasAllocation(accountId _: AccountId, coveringPeriod period: UInt32) async throws -> Bool {
        state.withLock { state in state.recordPeriod.map { $0 >= period } ?? false }
    }

    func ensureFunded(
        accountId: AccountId,
        priority: AllowanceRecord.Priority,
        domain _: AllowanceRecord.Domain
    ) async throws {
        let shouldHold = state.withLock { state in
            state.calls.append(accountId)
            state.priorities.append(priority)
            defer { state.holdsNext = false }
            return state.holdsNext
        }

        while shouldHold, !state.withLock({ $0.released }) {
            await Task.yield()
        }

        let failure: Error? = state.withLock { state in
            guard state.failuresLeft > 0 else {
                state.recordPeriod = state.periodOnFund
                return nil
            }
            state.failuresLeft -= 1
            return state.failure
        }

        if let failure {
            throw failure
        }
    }
}

struct FakeStatementAccountFundingError: Error {}
