import AsyncExtensions
import Foundation
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import SubstrateStorageSubscription

public protocol MembershipStatusWaiting {
    /// The ring index once `input` is an included member the ring status covers, re-checked on every
    /// change of its `Members` entry; nil when `timeout` passes first.
    func waitForStatus(of input: MembershipStatusInput, timeout: Duration) async throws -> MembersPallet.RingIndex?
}

public enum MembershipStatusWaiterError: Error {
    case subscriptionEnded
}

/// Subscribes to the member's `Members` entry and runs the full status check whenever it changes. The
/// ring keys page is written in the block that includes the member, so the entry is the one signal to
/// watch; the check itself still reads the ring status, so a not-yet-covered position keeps waiting.
public final class MembershipStatusWaiter: MembershipStatusWaiting, @unchecked Sendable {
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol
    private let statusChecker: MembershipStatusChecking
    private let clock: any Clock<Duration>

    public init(
        connection: JSONRPCEngine,
        runtimeCodingService: RuntimeCodingServiceProtocol,
        statusChecker: MembershipStatusChecking,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.connection = connection
        self.runtimeCodingService = runtimeCodingService
        self.statusChecker = statusChecker
        self.clock = clock
    }

    public func waitForStatus(
        of input: MembershipStatusInput,
        timeout: Duration
    ) async throws -> MembersPallet.RingIndex? {
        do {
            return try await withTimeout(timeout, clock: clock) { [self] in
                try await firstStatus(of: input)
            }
        } catch is TimeoutError {
            return nil
        }
    }
}

private extension MembershipStatusWaiter {
    func firstStatus(of input: MembershipStatusInput) async throws -> MembersPallet.RingIndex {
        let request = DoubleMapSubscriptionRequest(
            storagePath: MembersPallet.Storage.members(),
            localKey: "",
            keyParamClosure: {
                (
                    BytesCodable(wrappedValue: input.collection),
                    BytesCodable(wrappedValue: input.memberKey)
                )
            }
        )

        let stream: AnyAsyncSequence<BatchStorageSubscriptionSingleResult<MembersPallet.RingPosition?>>
        stream = CallbackBatchStorageSubscription.asyncStream(
            requests: [BatchStorageSubscriptionRequest(innerRequest: request, mappingKey: nil)],
            connection: connection,
            runtimeService: runtimeCodingService,
            logger: nil
        )

        for try await result in stream {
            // Only an included entry can pass the ring check, so onboarding updates are not re-read.
            guard result.value?.inclusion != nil else { continue }

            if let ringIndex = try await statusChecker.checkStatuses(of: [input], blockHash: nil)[input.memberKey] {
                return ringIndex
            }
        }

        // The stream also ends when the caller cancels; that is not a subscription failure.
        try Task.checkCancellation()
        throw MembershipStatusWaiterError.subscriptionEnded
    }
}
