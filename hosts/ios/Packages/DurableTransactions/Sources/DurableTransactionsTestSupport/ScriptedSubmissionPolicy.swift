import DurableTransactions
import ExtrinsicService
import Foundation
import os
import SubstrateSdk

/// A ``DurableSubmissionPolicy`` whose every answer is scripted, and which records what it was asked.
///
/// Both halves are driven separately: `canRetry` decides whether a failure becomes a retry, and
/// `prepareSubmission` decides what — if anything — a transaction is built into. A policy that neither
/// builds nor gives up is the ordinary case of one still waiting for the chain, so the default is to
/// answer nothing and be asked again.
public final class ScriptedSubmissionPolicy: DurableSubmissionPolicy, @unchecked Sendable {
    /// What `prepareSubmission` does with one call's worth of transactions.
    public enum Script: Sendable {
        /// Build every transaction asked about, each into bytes derived from its id.
        case buildAll
        /// Build nothing and answer nothing: they stay waiting.
        case waitAll
        /// Fail every transaction asked about for good.
        case giveUpAll
        /// Throw, which is never a verdict — the executor simply asks again after a backoff.
        case fail
        /// Decide per transaction; an id left out of the result stays waiting.
        case perId(@Sendable ([ScheduledDurableTx]) -> [DurableTxId: SubmissionPreparation])
    }

    public struct RetryQuestion: Sendable, Equatable {
        public let id: DurableTxId
        public let params: Data
        public let failure: DurableFailureKind
    }

    private struct State {
        var scripts: [Script] = []
        var lastScript: Script = .waitAll
        var retryAnswer: @Sendable (RetryQuestion) -> Bool = { _ in false }
        var prepareCalls: [[ScheduledDurableTx]] = []
        var retryQuestions: [RetryQuestion] = []
    }

    public let chainId: ChainId

    private let state = OSAllocatedUnfairLock(initialState: State())

    public init(chainId: ChainId = "test-chain") {
        self.chainId = chainId
    }

    /// Every call to `prepareSubmission`, in order, with the transactions it was given.
    public var prepareCalls: [[ScheduledDurableTx]] {
        state.withLock { $0.prepareCalls }
    }

    /// Every `canRetry` question asked, in order.
    public var retryQuestions: [RetryQuestion] {
        state.withLock { $0.retryQuestions }
    }

    /// Answers each successive `prepareSubmission` call from `scripts`, repeating the last one after the
    /// script runs out — so a test states only the rounds it cares about.
    public func script(_ scripts: Script...) {
        state.withLock { current in
            current.scripts = scripts.reversed()
            current.lastScript = scripts.last ?? .waitAll
        }
    }

    public func answerRetry(_ answer: @escaping @Sendable (RetryQuestion) -> Bool) {
        state.withLock { $0.retryAnswer = answer }
    }

    /// Retries every failure, or none.
    public func answerRetry(_ always: Bool) {
        answerRetry { _ in always }
    }

    public func canRetry(_ entry: DurableTxEntry, params: Data, failure: DurableFailureKind) async -> Bool {
        let question = RetryQuestion(id: entry.id, params: params, failure: failure)

        return state.withLock { current in
            current.retryQuestions.append(question)
            return current.retryAnswer(question)
        }
    }

    public func prepareSubmission(
        _ transactions: [ScheduledDurableTx]
    ) async throws -> [DurableTxId: SubmissionPreparation] {
        let script = state.withLock { current -> Script in
            current.prepareCalls.append(transactions)
            return current.scripts.popLast() ?? current.lastScript
        }

        switch script {
        case .buildAll:
            return transactions.reduce(into: [:]) { result, transaction in
                result[transaction.id] = .ready(Self.model(for: transaction.id))
            }
        case .waitAll:
            return [:]
        case .giveUpAll:
            return transactions.reduce(into: [:]) { $0[$1.id] = .giveUp }
        case .fail:
            throw ScriptedPolicyError.scripted
        case let .perId(decide):
            return decide(transactions)
        }
    }

    /// Distinct bytes per transaction, so an attempt can be told from the one it replaced. Varying
    /// `attempt` is what a rebuild does: the same transaction, different bytes.
    public static func model(for id: DurableTxId, attempt: Int = 0) -> ExtrinsicBuiltModel {
        .fixture(payload: "\(id.uuidString)-\(attempt)")
    }
}

public enum ScriptedPolicyError: Error {
    case scripted
}
