import Foundation
import SubstrateSdk
import ExtrinsicService
import SubstrateOperation
import ChainStore
import SDKLogger

public final class ValidatingExtrinsicSubmitter: @unchecked Sendable {
    private let base: ExtrinsicSubmitting
    private let validationApi: TaggedTransactionQueueApiProtocol
    private let recovery: ExtrinsicSubmissionRecovering
    private let blockInfoProvider: BlockInfoProviding
    private let chainId: ChainId
    /// How far the watch follows before treating a status as terminal. `.inBlock` completes on
    /// inclusion; `.finalized` keeps forwarding `inBlock` and completes only on finality — required
    /// by callers (e.g. coinage durability) that must observe the finalized outcome.
    private let trackingTill: ExtrinsicTrackingTill
    private let logger: SDKLoggerProtocol?

    private let registry = WatchHandleRegistry()

    public init(
        base: ExtrinsicSubmitting,
        validationApi: TaggedTransactionQueueApiProtocol,
        recovery: ExtrinsicSubmissionRecovering,
        blockInfoProvider: BlockInfoProviding,
        chainId: ChainId,
        trackingTill: ExtrinsicTrackingTill = .inBlock,
        logger: SDKLoggerProtocol?
    ) {
        self.base = base
        self.validationApi = validationApi
        self.recovery = recovery
        self.blockInfoProvider = blockInfoProvider
        self.chainId = chainId
        self.trackingTill = trackingTill
        self.logger = logger
    }
}

// MARK: - ExtrinsicSubmitting

extension ValidatingExtrinsicSubmitter: ExtrinsicSubmitting {
    // Batches are nonce-sequenced: resubmitting one extrinsic mid-batch would strand the rest,
    // so validation and recovery do not apply on this path.
    public func submit(
        builtExtrinsics: [ExtrinsicBuiltModel],
        completion: @escaping ExtrinsicSubmitResultsClosure
    ) {
        base.submit(builtExtrinsics: builtExtrinsics, completion: completion)
    }

    public func submitAndSubscribe(
        builtExtrinsic: ExtrinsicBuiltModel,
        runningIn queue: DispatchQueue,
        subscriptionIdClosure: @escaping ExtrinsicSubscriptionIdClosure,
        notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure
    ) {
        guard let handle = registry.makeHandle() else {
            logger?.error("No subscription handle available on \(chainId) — refusing submission")
            queue.async { notificationClosure(.failure(NoAvailableHandleError())) }
            return
        }

        guard subscriptionIdClosure(handle.id) else {
            registry.removeHandle(handle)
            return
        }

        let task = Task { [weak self] in
            guard let self else { return }

            await runSubmitAndRecover(
                builtExtrinsic: builtExtrinsic,
                queue: queue,
                notificationClosure: notificationClosure,
                handle: handle
            )
        }

        registry.assign(task: task, to: handle)
    }

    public func cancelExtrinsicWatch(for identifier: UInt16) {
        guard let currentId = registry.cancel(id: identifier) else { return }

        base.cancelExtrinsicWatch(for: currentId)
    }
}

// MARK: - Submit

private extension ValidatingExtrinsicSubmitter {
    enum SubmitOnceOutcome {
        case finished
        case recoverable(ExtrinsicSubmissionFailure, terminalToEmit: SubscriptionStatusResult)
    }

    typealias SubscriptionStatusResult = Result<ExtrinsicSubscribedStatusModel, Error>

    func runSubmitAndRecover(
        builtExtrinsic: ExtrinsicBuiltModel,
        queue: DispatchQueue,
        notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure,
        handle: WatchHandleRegistry.Handle
    ) async {
        var current = builtExtrinsic
        var isResubmission = false
        var hasSubmitted = false

        loop: while !Task.isCancelled {
            let outcome: SubmitOnceOutcome

            if !isResubmission, await !isValidForSubmission(current) {
                outcome = .recoverable(
                    .preSubmissionValidation,
                    terminalToEmit: .failure(PreSubmissionValidationFailedError())
                )
            } else {
                outcome = await submitOnce(
                    builtExtrinsic: current,
                    queue: queue,
                    suppressCreated: hasSubmitted,
                    notificationClosure: notificationClosure,
                    handle: handle
                )
                hasSubmitted = true
            }

            switch outcome {
            case .finished:
                break loop
            case let .recoverable(cause, terminal):
                logger?.warning("Submission attempt failed on \(chainId): \(cause)")
                switch await recovery.recover(builtExtrinsic: current, failure: cause) {
                case .abort:
                    if !Task.isCancelled {
                        emit(terminal, on: queue, to: notificationClosure)
                    }
                    break loop
                case let .resubmit(next):
                    current = next
                    isResubmission = true
                }
            }
        }

        registry.removeHandle(handle)
    }

    func submitOnce(
        builtExtrinsic: ExtrinsicBuiltModel,
        queue: DispatchQueue,
        suppressCreated: Bool,
        notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure,
        handle: WatchHandleRegistry.Handle
    ) async -> SubmitOnceOutcome {
        let (stream, continuation) = AsyncStream<SubscriptionStatusResult>.makeStream()

        registry.beginAttempt(handle)

        base.submitAndSubscribe(
            builtExtrinsic: builtExtrinsic,
            runningIn: queue,
            subscriptionIdClosure: { [weak self] realId in
                guard let self else { return false }

                if let staleId = registry.updateSubscriptionId(realId, handle: handle) {
                    base.cancelExtrinsicWatch(for: staleId)
                }

                return true
            },
            notificationClosure: { result in
                continuation.yield(result)
            }
        )

        var outcome: SubmitOnceOutcome = .finished

        await withTaskCancellationHandler {
            consume: for await result in stream {
                switch classify(result, suppressCreated: suppressCreated) {
                case .ignore:
                    continue
                case let .forward(status):
                    emit(status, on: queue, to: notificationClosure)
                case let .terminal(status):
                    emit(status, on: queue, to: notificationClosure)
                    break consume
                case let .recover(cause, terminal):
                    outcome = .recoverable(cause, terminalToEmit: terminal)
                    break consume
                }
            }
        } onCancel: {
            continuation.finish()
        }

        continuation.finish()

        if let currentId = registry.takeSubscriptionId(handle) {
            base.cancelExtrinsicWatch(for: currentId)
        }

        return outcome
    }
}

// MARK: - Status

private extension ValidatingExtrinsicSubmitter {
    enum StatusClassification {
        case ignore
        case forward(SubscriptionStatusResult)
        case terminal(SubscriptionStatusResult)
        case recover(ExtrinsicSubmissionFailure, terminal: SubscriptionStatusResult)
    }

    func classify(_ result: SubscriptionStatusResult, suppressCreated: Bool) -> StatusClassification {
        switch result {
        case let .success(model):
            let update = model.statusUpdate
            // In `.finalized` mode an `inBlock` update is not terminal: it is forwarded below and the
            // watch keeps running until the block is finalized.
            if update.getTerminalBlockHash(trackingTill: trackingTill) != nil {
                return .terminal(result)
            }

            if case let .onChain(status) = update.extrinsicStatus, status.isRecoverablePoolRejection {
                return .recover(.txInvalidation, terminal: result)
            }
            if update.getFinalExtrinsicFailure() != nil {
                return .terminal(result)
            }
            if suppressCreated, case .created = update.extrinsicStatus {
                return .ignore
            }
            return .forward(result)
        case let .failure(error):
            return .recover(.submission(error), terminal: .failure(error))
        }
    }

    func isValidForSubmission(_ builtExtrinsic: ExtrinsicBuiltModel) async -> Bool {
        do {
            let blockHash = try await blockInfoProvider.fetchCurrentHash()
            let validity = try await validationApi.validateTransaction(
                chainId: chainId,
                source: .external,
                extrinsic: builtExtrinsic.scaleBody,
                at: blockHash
            )

            switch validity {
            case let .invalid(reason):
                logger?.error("Pre-submission validation: invalid (\(reason)) on \(chainId) — aborting submission")
                return false
            case .valid:
                return true
            case let .unknown(reason):
                logger?.warning("Pre-submission validation inconclusive (\(reason)) on \(chainId) — proceeding")
                return true
            }
        } catch {
            logger?.warning("Pre-submission validation call failed on \(chainId): \(error) — proceeding")
            return true
        }
    }

    func emit(
        _ result: SubscriptionStatusResult,
        on queue: DispatchQueue,
        to notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure
    ) {
        queue.async { notificationClosure(result) }
    }
}

// MARK: - Errors

/// Raised when the submitter refuses to broadcast an extrinsic because it failed pre-submission
/// validation — the bytes never reached the node. Public so a tracker can distinguish this
/// never-broadcast failure from a post-broadcast pool error and decide a terminal outcome safely.
public struct PreSubmissionValidationFailedError: LocalizedError {
    public init() {}
    public var errorDescription: String? { "Extrinsic failed pre-submission validation" }
}

private struct NoAvailableHandleError: LocalizedError {
    var errorDescription: String? { "No extrinsic subscription handle available" }
}

// MARK: - Helper

private extension RemoteExtrinsicStatus {
    var isRecoverablePoolRejection: Bool {
        self == .invalid || self == .dropped
    }
}
