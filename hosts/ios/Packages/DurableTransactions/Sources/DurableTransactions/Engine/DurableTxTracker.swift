import AsyncExtensions
import BackgroundExecution
@preconcurrency import ExtrinsicService
import ExtrinsicServiceExt
import Foundation
import os
@preconcurrency import SDKLogger
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt

/// Follows one already-built extrinsic from submission to a terminal outcome.
///
/// It proposes; it does not decide unilaterally. Every status change goes through the same
/// compare-and-set (`updateTxStatus`) the recovery pass uses, so the guards apply uniformly and a status
/// that moved underneath costs a proposal, nothing more.
///
/// It owns the transactions it watches, and ownership is one-shot: released exactly once, never taken
/// back — not on a resubmission (the injected submitter owns that), not on anything. Release drops the
/// transaction; `onRecovery` then fires only if it is still undecided, so the happy path never schedules
/// a pass to re-derive an answer that already exists.
public final class DurableTxTracker: Sendable {
    /// One extrinsic to follow: the built model, the ledger row it belongs to, and the chain-bound
    /// submitter that broadcasts it.
    public struct Submission: Sendable {
        public let model: ExtrinsicBuiltModel
        public let transactionId: DurableTxId
        public let chainId: ChainId
        public let submitter: any ExtrinsicSubmitting

        public init(
            model: ExtrinsicBuiltModel,
            transactionId: DurableTxId,
            chainId: ChainId,
            submitter: any ExtrinsicSubmitting
        ) {
            self.model = model
            self.transactionId = transactionId
            self.chainId = chainId
            self.submitter = submitter
        }

        /// The hash of the bytes this watch follows — the attempt every verdict it writes is about.
        public var txHash: Data? {
            try? model.extrinsic.fromHex().blake2b32()
        }
    }

    private let store: any DurableTxRepositoryProtocol
    private let chainFactory: any PinnedChainViewFactoryProtocol
    private let owned: DurableTxOwnershipSet
    private let verdictWriter: DurableVerdictWriter
    private let backgroundExecutor: any BackgroundExecuting
    private let logger: SDKLoggerProtocol?

    /// Longest gap between status updates before the transaction is handed back to the pass. About
    /// fifteen blocks against a mortality window, so it always fires while the extrinsic can still execute.
    private static let silenceTimeout: Duration = .seconds(30)

    /// The queue the submitter delivers status callbacks on.
    private static let submissionQueue = DispatchQueue(label: "io.paritytech.durable.tx.submission")

    public init(
        store: any DurableTxRepositoryProtocol,
        chainFactory: any PinnedChainViewFactoryProtocol,
        owned: DurableTxOwnershipSet,
        verdictWriter: DurableVerdictWriter,
        backgroundExecutor: any BackgroundExecuting,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.chainFactory = chainFactory
        self.owned = owned
        self.verdictWriter = verdictWriter
        self.backgroundExecutor = backgroundExecutor
        self.logger = logger
    }

    /// Submits and tracks to a terminal outcome in a detached task, holding a background-task assertion
    /// so tracking survives the app being folded. Fire-and-forget: the caller has already registered the
    /// transaction and taken ownership.
    ///
    /// On release, `onRecovery` runs only when the transaction is still live.
    public func track(_ submission: Submission, onRecovery: @escaping @Sendable () -> Void) {
        guard let txHash = submission.txHash else {
            logger?.error("Untrackable submission for \(submission.transactionId): no extrinsic hash")

            return
        }

        Task { [self] in
            await runFollow(submission, attempt: txHash)

            guard owned.release(submission.transactionId, txHash: txHash) else { return }

            if await needsRecovery(submission.transactionId) {
                onRecovery()
            }
        }
    }
}

// MARK: - Follow

private extension DurableTxTracker {
    /// One tracked event: a chain status, or a terminal submission failure the submitter reported after
    /// it declined to resubmit.
    enum TrackEvent {
        case status(ExtrinsicStatusUpdate)
        case submissionFailed(Error)
    }

    func runFollow(_ submission: Submission, attempt txHash: Data) async {
        do {
            try await backgroundExecutor.execute {
                await self.follow(submission, attempt: txHash)
            }
        } catch {
            logger?.error("Submission watch failed for \(submission.transactionId): \(error)")
        }
    }

    /// Recovery is for transactions nobody has decided. Keyed on what the entry now says rather than on
    /// why the watch ended. Unreadable status counts as needing recovery.
    func needsRecovery(_ id: DurableTxId) async -> Bool {
        guard let status = try? await store.getStatus(id) else { return true }

        // One waiting to be built is the executor's, and no pass could decide it.
        return status.awaitsVerdict
    }

    func follow(_ submission: Submission, attempt txHash: Data) async {
        let id = submission.transactionId
        guard let view = try? await chainFactory.pin(chainId: submission.chainId) else {
            logger?.debug("Couldn't pin view for submission watch \(id)")
            return
        }

        // A buffered channel the submitter's callbacks feed. `send`/`finish` are non-blocking; `next()`
        // is cancellation-safe, so a timed-out receive drops no buffered element.
        let events = AsyncBufferedChannel<TrackEvent>()
        let subscriptionId = OSAllocatedUnfairLock<UInt16?>(initialState: nil)

        logger?.debug("Submitting transaction: \(id)")

        submission.submitter.submitAndSubscribe(
            builtExtrinsic: submission.model,
            runningIn: Self.submissionQueue,
            subscriptionIdClosure: { subscription in
                subscriptionId.withLock { $0 = subscription }
                return true
            },
            notificationClosure: { result in
                switch result {
                case let .success(status):
                    events.send(.status(status.statusUpdate))
                case let .failure(error):
                    events.send(.submissionFailed(error))
                    events.finish()
                }
            }
        )

        await consume(events, transactionId: id, attempt: txHash, using: view)

        // Stop the underlying watch if it is still open — a silence timeout ended following before the
        // submitter reached a terminal. A no-op if the watch already finished.
        if let subscription = subscriptionId.withLock({ $0 }) {
            submission.submitter.cancelExtrinsicWatch(for: subscription)
        }
    }

    /// Receives each event under a per-element silence timeout; a timeout or the channel finishing ends
    /// following and hands the transaction back to the pass.
    func consume(
        _ events: AsyncBufferedChannel<TrackEvent>,
        transactionId id: DurableTxId,
        attempt txHash: Data,
        using view: any PinnedChainViewProtocol
    ) async {
        let iterator = events.makeAsyncIterator()
        while true {
            let received = try? await withTimeout(Self.silenceTimeout) { await iterator.next() }
            guard let event = received.flatMap({ $0 }) else {
                logger?.warning("Transaction tracking timeout: \(id)")
                break
            }

            let isComplete = await handle(event, transactionId: id, attempt: txHash, using: view)
            if isComplete {
                logger?.debug("Terminal event for: \(id)")
                break
            }
        }
        events.finish()
    }

    /// Maps one event onto a proposed verdict. Returns true when the transaction is done being watched.
    func handle(
        _ event: TrackEvent,
        transactionId id: DurableTxId,
        attempt txHash: Data,
        using view: any PinnedChainViewProtocol
    ) async -> Bool {
        logger?.debug("Handling event: \(event) transaction: \(id)")

        switch event {
        case let .submissionFailed(error):
            return await handleSubmissionFailed(id, attempt: txHash, error: error)
        case let .status(update):
            return await handleStatus(update, transactionId: id, attempt: txHash, using: view)
        }
    }

    /// A `PreSubmissionValidationFailedError` means the submitter refused to broadcast — the extrinsic
    /// was never sent, so a terminal `failure` is justified immediately. Any other error is raised after
    /// the bytes may already have reached the node, where the extrinsic can still be included: a terminal
    /// verdict must rest on finalized evidence, so nothing is proposed and the pass decides.
    func handleSubmissionFailed(_ id: DurableTxId, attempt txHash: Data, error: Error) async -> Bool {
        if error is PreSubmissionValidationFailedError {
            await propose(id, attempt: txHash, Verdict(status: .failure, successDetectedAt: nil, failure: .rejected))
        }
        return true
    }

    /// Maps a chain status update onto a proposed verdict. Returns true when watching is done.
    func handleStatus(
        _ update: ExtrinsicStatusUpdate,
        transactionId id: DurableTxId,
        attempt txHash: Data,
        using view: any PinnedChainViewProtocol
    ) async -> Bool {
        guard case let .onChain(remote) = update.extrinsicStatus else {
            // `.created` carries no chain information.
            return false
        }

        switch remote {
        case .future,
             .ready,
             .broadcast:
            // Pre-inclusion states carry no evidence; they must not lower a transaction that already has
            // some (a resubmission can put one behind an inclusion).
            return false

        case let .inBlock(blockHash):
            await handleInBlock(blockHash: blockHash, transactionId: id, attempt: txHash, using: view)
            return false

        case let .retracted(blockHash):
            await clearRecordIfItNames(id, attempt: txHash, blockHash: blockHash)
            return false

        case let .finalized(blockHash):
            await handleFinalized(blockHash: blockHash, transactionId: id, attempt: txHash, using: view)
            return true

        case .dropped,
             .invalid,
             .unsurped,
             .finalityTimeout,
             .other:
            // Recovery has already had its chance to resubmit by the time these surface; the pass decides.
            return true
        }
    }
}

// MARK: - Verdicts

private extension DurableTxTracker {
    /// Not finalized, so a terminal verdict must not rest on it: success is recorded as `pendingSuccess`
    /// for the pass to finalize; a failure here proposes nothing.
    func handleInBlock(
        blockHash: String,
        transactionId id: DurableTxId,
        attempt txHash: Data,
        using view: any PinnedChainViewProtocol
    ) async {
        guard let block = await blockOf(blockHash, using: view) else {
            return
        }

        switch await dispatchOutcome(blockHash: blockHash, transactionId: id, using: view) {
        case .present(.succeeded):
            await propose(id, attempt: txHash, Verdict(status: .pendingSuccess, successDetectedAt: block))
        case let .present(.failed(reason)):
            // Nothing is proposed — the block is not finalized — but the reason is the only place the
            // chain ever states why, and by finality the events have long scrolled past.
            logger?.error("Dispatch failed in block \(blockHash) for \(id): \(reason ?? "unknown error")")
        case .absent,
             .failedRead:
            break
        }
    }

    func handleFinalized(
        blockHash: String,
        transactionId id: DurableTxId,
        attempt txHash: Data,
        using view: any PinnedChainViewProtocol
    ) async {
        switch await dispatchOutcome(blockHash: blockHash, transactionId: id, using: view) {
        case .present(.succeeded):
            let block = await blockOf(blockHash, using: view)
            await propose(id, attempt: txHash, Verdict(status: .finalizedSuccess, successDetectedAt: block))
        case let .present(.failed(reason)):
            logger?.error(
                "Dispatch failed at finality in block \(blockHash) for \(id): \(reason ?? "unknown error")"
            )
            await propose(
                id,
                attempt: txHash,
                Verdict(status: .failure, successDetectedAt: nil, failure: .dispatchFailed)
            )
        case .absent,
             .failedRead:
            // Unreadable outcome: record nothing and let the pass decide from state.
            break
        }
    }

    /// The record is cleared only when it names the retracted block; the status is lowered with it,
    /// because leaving `pendingSuccess` on a block that no longer exists would keep the domain's effects
    /// trusted for a whole mortality window on nothing.
    func clearRecordIfItNames(_ id: DurableTxId, attempt txHash: Data, blockHash: String) async {
        guard let hash = try? blockHash.fromHex(),
              let entry = try? await store.getEntry(id: id),
              entry.successDetectedAt?.hash == hash
        else { return }

        await propose(id, attempt: txHash, Verdict(status: .pending, successDetectedAt: nil))
    }

    /// A terminal row is never rewritten, so a late event cannot un-fail a failed transaction; the
    /// compare-and-set then covers a status that moved since it was read. A row whose attempt is no
    /// longer the one this watch follows is not this watch's to write: those bytes were proven unable to
    /// land and the transaction was built again.
    func propose(_ id: DurableTxId, attempt txHash: Data, _ verdict: Verdict) async {
        guard let observed = try? await store.getEntry(id: id),
              observed.status.awaitsVerdict,
              observed.attempt?.txHash == txHash
        else { return }

        do {
            logger?.debug("Proposing \(verdict.status) for id: \(id)")
            try await verdictWriter.write(observed, verdict)
        } catch {
            logger?.error("Proposal write failed for \(id) to \(verdict.status): \(error)")
        }
    }

    func dispatchOutcome(
        blockHash: String,
        transactionId id: DurableTxId,
        using view: any PinnedChainViewProtocol
    ) async -> ReadResult<DispatchOutcome> {
        guard
            let entry = try? await store.getEntry(id: id),
            let block = await blockOf(blockHash, using: view)
        else { return .failedRead }

        guard let attempt = entry.attempt else { return .failedRead }

        return await view.dispatchOutcome(txHash: attempt.txHash, at: block)
    }

    func blockOf(_ blockHash: String, using view: any PinnedChainViewProtocol) async -> BlockRef? {
        guard let hash = try? blockHash.fromHex() else { return nil }
        return await view.blockRef(forHash: hash).value
    }
}
