import Foundation
import os
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// Opening the overlay on the host's own behalf, and what the screen it was
/// opened from makes of the answer.
struct FundingOpeningTests {
    @Test
    func aStartedSessionIsStartedAndADismissalIsNot() {
        #expect(isStarted(FundingOpened(.success("fs_1"))))
        #expect(isNotStarted(FundingOpened(.success(nil))))
    }

    @Test
    func aCancelledOpeningSaysNothingAndAFailureIsShown() {
        guard case .cancelled = FundingOpened(.failure(CancellationError())) else {
            Issue.record("a cancellation is not cancelled")
            return
        }
        guard case let .failed(error) = FundingOpened(.failure(FundingOpeningError.refused)) else {
            Issue.record("a failure is not failed")
            return
        }
        #expect(error as? FundingOpeningError == .refused)
    }

    @Test
    func withoutARuntimeItCannotOpen() async {
        let pending = PendingRecorder(shows: false)
        let opener = ServiceFundingOpener(serviceProvider: nil, showPending: pending.show)

        await #expect(throws: FundingOpeningError.runtimeUnavailable) {
            try await opener.openFunding(direction: .in)
        }
        #expect(pending.asked.isEmpty)
    }

    @Test
    func sessionsInFlightAreShownInsteadOfOpening() async throws {
        let runtime = FailingRuntime(error: Unavailable())
        let pending = PendingRecorder(shows: true)
        let opener = ServiceFundingOpener(serviceProvider: runtime, showPending: pending.show)

        let session = try await opener.openFunding(direction: .out)

        #expect(session == nil)
        #expect(pending.asked == [.out])
        #expect(runtime.builds == 0)
    }

    @Test
    func aRuntimeThatCannotBeBuiltIsARefusal() async {
        let runtime = FailingRuntime(error: Unavailable())
        let opener = ServiceFundingOpener(serviceProvider: runtime, showPending: PendingRecorder(shows: false).show)

        await #expect(throws: FundingOpeningError.refused) {
            try await opener.openFunding(direction: .in)
        }
        #expect(runtime.builds == 1)
    }

    @Test
    func aCancellationIsPassedOnAsIs() async {
        let opener = ServiceFundingOpener(
            serviceProvider: FailingRuntime(error: CancellationError()),
            showPending: PendingRecorder(shows: false).show
        )

        await #expect(throws: CancellationError.self) {
            try await opener.openFunding(direction: .in)
        }
    }

    private func isStarted(_ opened: FundingOpened) -> Bool {
        if case .started = opened { return true }
        return false
    }

    private func isNotStarted(_ opened: FundingOpened) -> Bool {
        if case .notStarted = opened { return true }
        return false
    }
}

private struct Unavailable: Error {}

private final class PendingRecorder: Sendable {
    private let shows: Bool
    private let state = OSAllocatedUnfairLock<[FundingDirection]>(initialState: [])
    var asked: [FundingDirection] { state.withLock { $0 } }

    init(shows: Bool) {
        self.shows = shows
    }

    var show: @Sendable (FundingDirection) async -> Bool {
        { [self] direction in
            state.withLock { $0.append(direction) }
            return shows
        }
    }
}

/// A runtime that cannot be built, so neither can its funding service.
private final class FailingRuntime: FundingServiceProviding, @unchecked Sendable {
    private let error: Error
    private let state = OSAllocatedUnfairLock(initialState: 0)
    var builds: Int { state.withLock { $0 } }

    init(error: Error) {
        self.error = error
    }

    func fundingService() throws -> FundingServiceProtocol {
        state.withLock { $0 += 1 }
        throw error
    }

    func builtFundingService() -> AsyncStream<FundingServiceProtocol> {
        AsyncStream { $0.finish() }
    }
}
