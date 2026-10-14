import DurableTransactions
import ExtrinsicService
import Foundation
import os
import SubstrateSdk

/// A ``DurableAttemptStarting`` that records what it was asked to start and answers from a script,
/// standing in for the launcher so the executor can be driven without a chain.
public final class ScriptedAttemptStarter: DurableAttemptStarting, @unchecked Sendable {
    public struct Start: Sendable, Equatable {
        public let id: DurableTxId
        public let extrinsic: String
        public let chainId: ChainId
    }

    /// How one call answers.
    public enum Outcome: Sendable {
        /// The transaction was still waiting and its attempt was recorded.
        case started
        /// Something else took the transaction first, so nothing was started.
        case alreadyGone
        /// The engine refused the extrinsic outright — the executor abandons the row.
        case reject(DurableTxError)
        /// A transient failure; the executor retries after a backoff.
        case fail(any Error)
    }

    private struct State {
        var starts: [Start] = []
        var outcomes: [Outcome] = []
        var lastOutcome: Outcome = .started
        /// Set when the starter should write the attempt through to a store, as the launcher does.
        var store: (any DurableTxRepositoryProtocol)?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    public init(writingTo store: (any DurableTxRepositoryProtocol)? = nil) {
        state.withLock { $0.store = store }
    }

    public var starts: [Start] {
        state.withLock { $0.starts }
    }

    /// Answers each successive call from `outcomes`, repeating the last after the script runs out.
    public func script(_ outcomes: Outcome...) {
        state.withLock { current in
            current.outcomes = outcomes.reversed()
            current.lastOutcome = outcomes.last ?? .started
        }
    }

    public func startAttempt(
        id: DurableTxId,
        model: ExtrinsicBuiltModel,
        chainId: ChainId
    ) async throws -> Bool {
        let (outcome, store) = state.withLock { current -> (Outcome, (any DurableTxRepositoryProtocol)?) in
            current.starts.append(Start(id: id, extrinsic: model.extrinsic, chainId: chainId))
            return (current.outcomes.popLast() ?? current.lastOutcome, current.store)
        }

        switch outcome {
        case .started:
            // Mirrors the launcher: the row leaves `pendingSubmission` only by gaining an attempt, and
            // without that the executor would be handed the same transaction on the next round.
            guard let store else { return true }

            return try await store.startAttempt(id: id, attempt: DurableTxAttempt(from: model))
        case .alreadyGone:
            return false
        case let .reject(error):
            throw error
        case let .fail(error):
            throw error
        }
    }
}
