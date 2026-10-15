import AsyncExtensions
import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// How the payments bridge answers the core's synchronous status read from an
/// engine's status stream.
struct EngineStatusFollowersTests {
    private let key = EngineStatusKey(productId: "wallet.dot", id: Data(repeating: 7, count: 32))

    @Test
    func readsTheFirstStatusTheEngineSends() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let status = read(followers, key)
        await engine.waitForStreams(1)
        engine.send(1, isFinal: false)

        #expect(try await status == 1)
    }

    @Test
    func anIdTheEngineDoesNotHoldReadsAsNone() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let status = read(followers, key)
        await engine.waitForStreams(1)
        engine.fail(FakeEngine.NotFound())

        #expect(try await status == nil)
    }

    @Test
    func aFailingEngineReadsAsUnavailableWithoutItsError() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        let status = Task { [key] in try await read(followers, key) }
        await engine.waitForStreams(1)
        engine.fail(FakeEngine.StoreBroken())

        await #expect(throws: HostRejection.Rejected(reason: "status is unavailable")) { try await status.value }
    }

    @Test
    func anEngineThatDoesNotAnswerInTimeReadsAsUnavailable() async throws {
        let engine = FakeEngine()
        let followers = engine.followers(timeout: 0.2)

        await #expect(throws: HostRejection.Rejected(reason: "status is unavailable")) {
            try await read(followers, key)
        }
    }

    @Test
    func aFinalStatusIsReadAgainWithoutTheEngine() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        await engine.waitForStreams(1)
        engine.send(2, isFinal: true)
        #expect(try await first == 2)

        #expect(try await read(followers, key) == 2)
        #expect(engine.streamCount == 1)
    }

    @Test
    func aStreamEndingBeforeAFinalStatusIsFollowedAgain() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        await engine.waitForStreams(1)
        engine.send(1, isFinal: false)
        engine.end()
        #expect(try await first == 1)
        await engine.waitForEnded(1)

        async let second = read(followers, key)
        await engine.waitForStreams(2)
        engine.send(3, isFinal: true)
        #expect(try await second == 3)
    }

    @Test
    func readsWaitingTogetherShareOneStream() async throws {
        let engine = FakeEngine()
        let followers = engine.followers()

        async let first = read(followers, key)
        async let second = read(followers, key)
        await engine.waitForStreams(1)
        try await Task.sleep(for: .milliseconds(50))
        engine.send(4, isFinal: false)

        #expect(try await first == 4)
        #expect(try await second == 4)
        #expect(engine.streamCount == 1)
    }
}

/// `current` blocks its thread, as the core's read does, so it runs off the
/// cooperative pool.
private func read(_ followers: EngineStatusFollowers<Int>, _ key: EngineStatusKey) async throws -> Int? {
    try await withCheckedThrowingContinuation { continuation in
        DispatchQueue.global().async {
            continuation.resume(with: Result { try followers.current(key) })
        }
    }
}

/// An engine whose status stream the test drives.
private final class FakeEngine: @unchecked Sendable {
    struct NotFound: Error {}
    struct StoreBroken: Error {}

    private let lock = NSLock()
    private var continuations: [AsyncThrowingStream<(Int, Bool), Error>.Continuation] = []
    private var ended = 0

    var streamCount: Int {
        lock.withLock { continuations.count }
    }

    func followers(timeout: TimeInterval = EngineStatusFollowing.firstStatusTimeout) -> EngineStatusFollowers<Int> {
        EngineStatusFollowers(
            statuses: { [self] _ in
                let (stream, continuation) = AsyncThrowingStream<(Int, Bool), Error>.makeStream()
                continuation.onTermination = { [self] _ in lock.withLock { ended += 1 } }
                lock.withLock { continuations.append(continuation) }
                return stream.eraseToAnyAsyncSequence()
            },
            isNotFound: { $0 is NotFound },
            notify: { _, _ in },
            logger: MockLogger(),
            timeout: timeout
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

    func waitForStreams(_ count: Int) async {
        while streamCount < count {
            try? await Task.sleep(for: .milliseconds(5))
        }
    }

    func waitForEnded(_ count: Int) async {
        while lock.withLock({ ended }) < count {
            try? await Task.sleep(for: .milliseconds(5))
        }
        try? await Task.sleep(for: .milliseconds(20))
    }

    private var latest: AsyncThrowingStream<(Int, Bool), Error>.Continuation? {
        lock.withLock { continuations.last }
    }
}
