import AsyncExtensions
import Foundation
import FoundationExt
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How the payments bridge answers the core's synchronous status read from an
/// engine's status stream, and what it pushes to the core.
@Suite(.timeLimit(.minutes(1)))
struct EngineStatusFollowersTests {
    private let key = EngineStatusKey(productId: "wallet.dot", id: Data(repeating: 7, count: 32))

    @Test
    func readsTheFirstStatusTheEngineSends() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let status = read(followers, key)
        try await engine.waitForStreams(1)
        engine.send(1, isFinal: false)

        #expect(try await status == 1)
    }

    @Test
    func pushesEachStatusOnceAndNothingAfterAFinalOne() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        followers.follow(key)
        try await engine.waitForStreams(1)
        engine.send(1, isFinal: false)
        engine.send(2, isFinal: false)
        engine.send(3, isFinal: true)
        engine.send(4, isFinal: false)
        try await eventually { engine.pushed.count >= 3 }
        try await Task.sleep(for: .milliseconds(50))

        #expect(engine.pushed == [1, 2, 3])
    }

    @Test
    func anIdTheEngineDoesNotHoldReadsAsNone() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let status = read(followers, key)
        try await engine.waitForStreams(1)
        engine.fail(FakeEngine.NotFound())

        #expect(try await status == nil)
    }

    @Test
    func aFailingEngineReadsAsUnavailableWithoutItsError() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        let status = Task { [key] in try await read(followers, key) }
        try await engine.waitForStreams(1)
        engine.fail(FakeEngine.StoreBroken())

        await #expect(throws: HostRejection.Rejected(reason: "status is unavailable")) { try await status.value }
    }

    @Test
    func aFailureAfterAStatusPushesAbandonedAndIsFollowedAgain() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        try await engine.waitForStreams(1)
        engine.send(1, isFinal: false)
        #expect(try await first == 1)
        engine.fail(FakeEngine.StoreBroken())
        try await eventually { !followers.isFollowing(key) }

        #expect(engine.pushed == [1, FakeEngine.abandoned])
    }

    @Test
    func anEngineThatDoesNotAnswerInTimeReadsAsPendingThenPushesTheRealStatus() async throws {
        let engine = FakeEngine()
        let followers = engine.followers(timeout: pendingWindow.timeInterval)

        #expect(try await read(followers, key) == FakeEngine.pending)

        try await engine.waitForStreams(1)
        engine.send(5, isFinal: false)
        try await eventually { engine.pushed == [5] }
        #expect(try await read(followers, key) == 5)
    }

    @Test
    func anIdTheEngineDoesNotHoldAfterAPendingAnswerReadsAsNoneNextTime() async throws {
        let engine = FakeEngine()
        let followers = engine.followers(timeout: pendingWindow.timeInterval)

        #expect(try await read(followers, key) == FakeEngine.pending)
        try await engine.waitForStreams(1)
        engine.fail(FakeEngine.NotFound())
        try await eventually { !followers.isFollowing(key) }
        #expect(engine.pushed == [FakeEngine.abandoned])

        engine.forgetsIds = true
        #expect(try await read(followers, key) == nil)
        #expect(engine.pushed == [FakeEngine.abandoned])
    }

    @Test
    func anIdTheEngineDoesNotHoldBeforeAnythingWasReportedPushesNothing() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        followers.follow(key)
        try await engine.waitForStreams(1)
        engine.fail(FakeEngine.NotFound())
        try await eventually { !followers.isFollowing(key) }

        #expect(engine.pushed.isEmpty)
    }

    @Test
    func followingBeforeTheFirstReadAnswersFromThatStream() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        followers.follow(key)
        try await engine.waitForStreams(1)
        engine.send(6, isFinal: false)
        try await eventually { engine.pushed == [6] }

        #expect(try await read(followers, key) == 6)
        #expect(engine.streamCount == 1)
    }

    @Test
    func aFinalStatusIsReadAgainWithoutTheEngine() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        try await engine.waitForStreams(1)
        engine.send(2, isFinal: true)
        #expect(try await first == 2)

        #expect(try await read(followers, key) == 2)
        #expect(engine.streamCount == 1)
    }

    @Test
    func onlyTheMostRecentFinalStatusesAreKept() async throws {
        let engine = FakeEngine()
        let followers = engine.followers(finalCapacity: 2)
        let keys = (1 ... 3).map { EngineStatusKey(productId: "wallet.dot", id: Data(repeating: UInt8($0), count: 32)) }

        for (index, key) in keys.enumerated() {
            followers.follow(key)
            try await engine.waitForStreams(index + 1)
            engine.send(index, isFinal: true)
            try await eventually { engine.pushed.count == index + 1 }
        }
        try await eventually { !followers.isFollowing(keys[0]) }

        #expect(followers.isFollowing(keys[1]))
        #expect(followers.isFollowing(keys[2]))
    }

    @Test
    func aStreamEndingBeforeAFinalStatusIsFollowedAgain() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        try await engine.waitForStreams(1)
        engine.send(1, isFinal: false)
        #expect(try await first == 1)
        engine.end()
        try await eventually { !followers.isFollowing(key) }
        #expect(engine.pushed == [1, FakeEngine.abandoned])

        async let second = read(followers, key)
        try await engine.waitForStreams(2)
        engine.send(3, isFinal: true)
        #expect(try await second == 3)
    }

    @Test
    func readsWaitingTogetherShareOneStream() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        async let second = read(followers, key)
        try await engine.waitForStreams(1)
        engine.send(4, isFinal: false)

        #expect(try await first == 4)
        #expect(try await second == 4)
        #expect(engine.streamCount == 1)
    }
}

/// The read window for tests that expect `pending`. Their engine never answers before the first
/// read returns, so that read is deterministic; the window only has to let a later read see a
/// stream that fails or ends at once.
private let pendingWindow: Duration = .seconds(1)

/// `current` blocks its thread, as the core's read does, so it runs off the
/// cooperative pool.
private func read(_ followers: EngineStatusFollowers<Int>, _ key: EngineStatusKey) async throws -> Int? {
    try await withCheckedThrowingContinuation { continuation in
        DispatchQueue.global().async {
            continuation.resume(with: Result { try followers.current(key) })
        }
    }
}

private struct ConditionNotMet: Error {}

/// Waits for `condition`, failing the test after `timeout` instead of hanging it.
private func eventually(
    timeout: Duration = .hangGuard,
    _ condition: () -> Bool
) async throws {
    let clock = ContinuousClock()
    let deadline = clock.now + timeout
    while !condition() {
        guard clock.now < deadline else {
            Issue.record("condition not met within \(timeout)")
            throw ConditionNotMet()
        }
        try await Task.sleep(for: .milliseconds(5))
    }
}

/// An engine whose status streams the test drives, recording what the
/// followers push to the core.
private final class FakeEngine: @unchecked Sendable {
    struct NotFound: Error {}
    struct StoreBroken: Error {}

    static let pending = -1
    static let abandoned = -2

    private let lock = NSLock()
    private var continuations: [AsyncThrowingStream<(Int, Bool), Error>.Continuation] = []
    private var pushes: [Int] = []
    private var forgets = false

    /// When set, every new stream fails at once with ``NotFound``.
    var forgetsIds: Bool {
        get { lock.withLock { forgets } }
        set { lock.withLock { forgets = newValue } }
    }

    var streamCount: Int {
        lock.withLock { continuations.count }
    }

    var pushed: [Int] {
        lock.withLock { pushes }
    }

    /// Reads wait for the engine's first answer as long as the hang guard, so a status the test
    /// sends a moment after the read is never mistaken for an engine that did not answer.
    func followers(
        timeout: TimeInterval = Duration.hangGuard.timeInterval,
        finalCapacity: Int = EngineStatusFollowing.finalCapacity
    ) -> EngineStatusFollowers<Int> {
        EngineStatusFollowers(
            statuses: { [self] _ in
                if forgetsIds { throw NotFound() }
                let (stream, continuation) = AsyncThrowingStream<(Int, Bool), Error>.makeStream()
                lock.withLock { continuations.append(continuation) }
                return stream.eraseToAnyAsyncSequence()
            },
            isNotFound: { $0 is NotFound },
            notify: { [self] _, status in lock.withLock { pushes.append(status) } },
            pending: Self.pending,
            abandoned: Self.abandoned,
            logger: MockLogger(),
            timeout: timeout,
            finalCapacity: finalCapacity
        )
    }

    func send(_ status: Int, isFinal: Bool) {
        latest?.yield((status, isFinal))
    }

    func end() {
        latest?.finish()
    }

    func fail(_ error: Error) {
        latest?.finish(throwing: error)
    }

    func waitForStreams(_ count: Int) async throws {
        try await eventually { streamCount >= count }
    }

    private var latest: AsyncThrowingStream<(Int, Bool), Error>.Continuation? {
        lock.withLock { continuations.last }
    }
}
