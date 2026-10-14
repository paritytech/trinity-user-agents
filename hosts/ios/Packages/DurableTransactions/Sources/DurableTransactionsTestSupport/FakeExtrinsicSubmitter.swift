@preconcurrency import ExtrinsicService
import Foundation
import os
import SubstrateSdk

/// Ends the tracker's watch without deciding anything.
///
/// The tracker treats a submission failure that is *not* a `PreSubmissionValidationFailedError` as "give
/// up, let the pass decide" — it proposes no verdict and releases ownership. Emitting this after a
/// submission's configured statuses reproduces the net effect of an empty status flow (release, no
/// verdict) while exercising the real async tracker, and without waiting out the silence timeout.
public struct HarnessSubmissionEnded: Error {
    public init() {}
}

/// The subscription died or could not be opened — a non-pre-submission failure.
public struct HarnessSubscriptionLost: Error {
    public init() {}
}

/// A test-controlled stream of watch events for one submission. A scenario pushes chain statuses (or a
/// submission failure) into it, and the harness flushes them through the real tracker in order before
/// ending the watch.
public final class HarnessStatusStream: @unchecked Sendable {
    public enum Event {
        case status(ExtrinsicStatusUpdate)
        case failure(Error)
    }

    private let events = OSAllocatedUnfairLock(initialState: [Event]())

    public init() {}

    public func drain() -> [Event] {
        events.withLock { current in
            let snapshot = current
            current.removeAll()
            return snapshot
        }
    }

    public func inBlock(_ blockHash: Data, txHash: Data) {
        push(.inBlock(blockHash.toHex(includePrefix: true)), txHash: txHash)
    }

    public func finalized(_ blockHash: Data, txHash: Data) {
        push(.finalized(blockHash.toHex(includePrefix: true)), txHash: txHash)
    }

    public func retracted(_ blockHash: Data, txHash: Data) {
        push(.retracted(blockHash.toHex(includePrefix: true)), txHash: txHash)
    }

    public func ready(txHash: Data) { push(.ready, txHash: txHash) }

    public func dropped(txHash: Data) { push(.dropped, txHash: txHash) }

    public func failedToSubmit(_ error: Error) {
        events.withLock { $0.append(.failure(error)) }
    }

    /// A stream that models a subscription that could not be opened or died: it reports a submission
    /// failure that is not a pre-submission validation error, so the tracker releases to the pass.
    public static func failing(_ error: Error = HarnessSubscriptionLost()) -> HarnessStatusStream {
        let stream = HarnessStatusStream()
        stream.failedToSubmit(error)
        return stream
    }

    private func push(_ status: RemoteExtrinsicStatus, txHash: Data) {
        let update = ExtrinsicStatusUpdate(
            extrinsicHash: txHash.toHex(includePrefix: true),
            extrinsicStatus: .onChain(status)
        )
        events.withLock { $0.append(.status(update)) }
    }
}

/// A fake ``ExtrinsicSubmitting`` that accepts a submission and hands back a subscription id, then
/// *parks* the watch until ``releaseAll()`` flushes it — so a transaction stays submission-owned exactly
/// until the harness releases it. On release it emits the submission's configured statuses (via
/// ``streamFactory``) in order, then a non-pre-submission failure that the tracker treats as "give up,
/// let the pass decide" unless a status already reached a terminal.
public final class FakeExtrinsicSubmitter: ExtrinsicSubmitting, @unchecked Sendable {
    private struct Parked {
        let queue: DispatchQueue
        let notify: ExtrinsicSubscriptionStatusClosure
        let stream: HarnessStatusStream?
    }

    private struct State {
        var submissions = 0
        var nextSubscriptionId: UInt16 = 1
        var cancelled: Set<UInt16> = []
        var parked: [Parked] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    /// Supplies the status stream for the n-th submission (0-based); `nil` means no configured statuses,
    /// so the watch just ends. Set by a scenario before registering.
    public var streamFactory: (@Sendable (Int) -> HarnessStatusStream?)?

    public init() {}

    /// How many extrinsics have been submitted, so a scenario can assert no resubmission followed.
    public var submissionCount: Int { state.withLock { $0.submissions } }

    /// Flushes every parked watch — its configured statuses, then a terminal — so the trackers holding
    /// them release.
    public func releaseAll() {
        let parked = state.withLock { current -> [Parked] in
            let snapshot = current.parked
            current.parked.removeAll()
            return snapshot
        }
        for item in parked {
            item.queue.async {
                for event in item.stream?.drain() ?? [] {
                    switch event {
                    case let .status(update):
                        item.notify(.success(ExtrinsicSubscribedStatusModel(statusUpdate: update, sender: .none)))
                    case let .failure(error):
                        item.notify(.failure(error))
                    }
                }
                // Ends any watch a status did not already terminate. A non-pre-submission failure, so the
                // tracker proposes nothing and releases.
                item.notify(.failure(HarnessSubmissionEnded()))
            }
        }
    }

    public func submit(builtExtrinsics: [ExtrinsicBuiltModel], completion: @escaping ExtrinsicSubmitResultsClosure) {
        completion(builtExtrinsics.map { _ in .failure(HarnessSubmissionEnded()) })
    }

    public func submitAndSubscribe(
        builtExtrinsic _: ExtrinsicBuiltModel,
        runningIn queue: DispatchQueue,
        subscriptionIdClosure: @escaping ExtrinsicSubscriptionIdClosure,
        notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure
    ) {
        let subscriptionId = state.withLock { current -> UInt16 in
            let index = current.submissions
            current.submissions += 1
            let value = current.nextSubscriptionId
            current.nextSubscriptionId += 1
            current.parked.append(Parked(queue: queue, notify: notificationClosure, stream: streamFactory?(index)))
            return value
        }

        queue.async { _ = subscriptionIdClosure(subscriptionId) }
    }

    public func cancelExtrinsicWatch(for identifier: UInt16) {
        state.withLock { current in _ = current.cancelled.insert(identifier) }
    }
}

/// A minimal built extrinsic for a given hash. The tracker never inspects its sender or mortality — it
/// only hands the model to the submitter — so an immortal model with no sender is enough.
public func harnessBuiltModel(hex: String) -> ExtrinsicBuiltModel {
    ExtrinsicBuiltModel(extrinsic: hex, sender: .none, mortality: .immortal)
}
