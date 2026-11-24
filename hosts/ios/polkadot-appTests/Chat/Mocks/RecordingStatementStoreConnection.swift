import AsyncExtensions
import Foundation
import os
import StatementStore
import SubstrateSdk

final class RecordingStatementStoreConnection: StatementStoreConnecting, @unchecked Sendable {
    private struct State {
        var submitCount = 0
        var submittedSenders: [Data?] = []
        var fetchCount = 0
        var failures: [Error]
        var fetchFailures: [Error]
    }

    private let state: OSAllocatedUnfairLock<State>
    private let stored: [Data]

    var submitCount: Int { state.withLock { $0.submitCount } }
    var submittedSenders: [Data?] { state.withLock { $0.submittedSenders } }
    var fetchCount: Int { state.withLock { $0.fetchCount } }

    /// Each submit and each fetch fails with the next error queued for it, then succeeds.
    init(failures: [Error] = [], stored: [Data] = [], fetchFailures: [Error] = []) {
        state = OSAllocatedUnfairLock(uncheckedState: State(failures: failures, fetchFailures: fetchFailures))
        self.stored = stored
    }

    func submitStatement(with builder: StatementSubmitParametersBuilding) async throws {
        let parameters = try builder.build()
        let sender = try Statement(scaleDecoder: ScaleDecoder(data: parameters.encodedStatement)).getSenderAccountId()

        let failure: Error? = state.withLock { state in
            state.submitCount += 1
            state.submittedSenders.append(sender)
            return state.failures.isEmpty ? nil : state.failures.removeFirst()
        }

        if let failure {
            throw failure
        }
    }

    func fetchStatements(with _: TopicFilter) async throws -> [Data] {
        let failure: Error? = state.withLock { state in
            state.fetchCount += 1
            return state.fetchFailures.isEmpty ? nil : state.fetchFailures.removeFirst()
        }

        if let failure {
            throw failure
        }

        return stored
    }

    func subscribeStatements(with _: TopicFilter) -> AnyAsyncSequence<StatementsPage> {
        AsyncThrowingStream<StatementsPage, Error> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
