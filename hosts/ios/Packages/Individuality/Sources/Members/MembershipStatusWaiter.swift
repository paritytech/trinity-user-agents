import Foundation
import StructuredConcurrency
import SubstrateSdk

public protocol MembershipStatusWaiting {
    /// The ring index once `input` is an included member whose key the ring root covers. Waits until
    /// then or until the caller cancels.
    func waitForStatus(of input: MembershipStatusInput) async throws -> MembersPallet.RingIndex
    func waitForStatus(of input: MembershipStatusInput, timeout: Duration) async throws -> MembersPallet.RingIndex?
}

public enum MembershipStatusWaiterError: Error {
    case subscriptionEnded
}

/// Inclusion is two chain writes on two storage items: `onboard_people` marks the `Members` entry
/// included, and a later `build_ring` bakes the key into the root by advancing `RingKeysStatus`.
/// Neither write touches the other item, so each step is awaited on its own subscription.
public final class MembershipStatusWaiter: MembershipStatusWaiting, @unchecked Sendable {
    private let subscriber: any MembershipSubscribing
    private let clock: any Clock<Duration>

    public init(
        subscriber: any MembershipSubscribing,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.subscriber = subscriber
        self.clock = clock
    }

    public func waitForStatus(
        of input: MembershipStatusInput,
        timeout: Duration
    ) async throws -> MembersPallet.RingIndex? {
        do {
            return try await withTimeout(timeout, clock: clock) { [self] in
                try await waitForStatus(of: input)
            }
        } catch is TimeoutError {
            return nil
        }
    }

    public func waitForStatus(of input: MembershipStatusInput) async throws -> MembersPallet.RingIndex {
        let inclusion = try await awaitInclusion(of: input)
        try await awaitRootCoverage(of: inclusion, in: input.collection)
        return inclusion.ringIndex
    }
}

private extension MembershipStatusWaiter {
    func awaitInclusion(of input: MembershipStatusInput) async throws -> MembersPallet.RingPosition.Included {
        for try await position in subscriber.memberPositions(of: input) {
            if let inclusion = position?.inclusion {
                return inclusion
            }
        }

        try endedWithoutResult()
    }

    func awaitRootCoverage(
        of inclusion: MembersPallet.RingPosition.Included,
        in collection: MembersPallet.CollectionIdentifier
    ) async throws {
        let keysPerPage = try await subscriber.ringKeysPageSize()
        let statuses = subscriber.ringKeysStatuses(collection: collection, ringIndex: inclusion.ringIndex)

        for try await status in statuses where status?.includesKey(at: inclusion, keysPerPage: keysPerPage) == true {
            return
        }

        try endedWithoutResult()
    }

    /// A stream also ends when the caller cancels; that is not a subscription failure.
    func endedWithoutResult() throws -> Never {
        try Task.checkCancellation()
        throw MembershipStatusWaiterError.subscriptionEnded
    }
}
