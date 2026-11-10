import Testing
import Foundation
import AsyncExtensions
import os
@testable import Coinage

struct IncomingPaymentContextTests {
    @Test func processSeedsDetecting() async throws {
        let context = IncomingPaymentContext(logger: StubLogger())

        // A run that reports nothing — the subject should still be seeded `.detecting`.
        await context.process(groupId: "g1") { Task {} }

        let stream = try await context.liveStatusStream(for: "g1") { .notClaimed }
        for try await status in stream {
            #expect(status == .detecting)
            break
        }
    }

    @Test func dedupsByGroupId() async {
        let context = IncomingPaymentContext(logger: StubLogger())
        // `process` invokes the runner synchronously, so the count is exact once both calls return —
        // no waiting that a dropped guard could slip past.
        let runs = OSAllocatedUnfairLock(initialState: 0)

        await context.process(groupId: "g1") { runs.withLock { $0 += 1 }; return Task {} }
        await context.process(groupId: "g1") { runs.withLock { $0 += 1 }; return Task {} }

        #expect(runs.withLock { $0 } == 1)
    }

    @Test(.timeLimit(.minutes(1)))
    func reportEmitsStatusesButContextNeverPersists() async throws {
        let context = IncomingPaymentContext(logger: StubLogger())

        await context.process(groupId: "g1") {
            Task {
                await context.report(.claiming, for: "g1")
                await context.report(.claimed(finalized: true), for: "g1")
            }
        }

        let stream = try await context.liveStatusStream(for: "g1") { .notClaimed }
        var last: IncomingPaymentStatus?
        for try await status in stream {
            last = status
            if status.isTerminal { break }
        }
        #expect(last == .claimed(finalized: true))
    }

    @Test func fallbackSeedsStreamWhenNoLiveStreamExists() async throws {
        let context = IncomingPaymentContext(logger: StubLogger())

        // No task ran for this group, so the fallback value is seeded and returned.
        let stream = try await context.liveStatusStream(for: "never-seen") { .notClaimed }
        for try await status in stream {
            #expect(status == .notClaimed)
            break
        }
    }

    @Test func fallbackErrorPropagates() async {
        struct Boom: Error {}
        let context = IncomingPaymentContext(logger: StubLogger())

        await #expect(throws: Boom.self) {
            _ = try await context.liveStatusStream(for: "never-seen") { throw Boom() }
        }
    }

    @Test func finishFreesSlotForQueuedWork() async throws {
        let context = IncomingPaymentContext(maxConcurrent: 1, logger: StubLogger())
        let started = Counter()

        await context.process(groupId: "g1") { Task { await started.increment() } }
        // At capacity — this one queues behind g1.
        await context.process(groupId: "g2") { Task { await started.increment() } }

        try await waitUntil { await started.value() >= 1 }
        await context.finish(groupId: "g1")

        try await waitUntil { await started.value() == 2 }
        #expect(await started.value() == 2)
    }

    private func waitUntil(
        _ condition: @escaping () async -> Bool,
        timeout: Duration = .seconds(100)
    ) async throws {
        let deadline = ContinuousClock.now.advanced(by: timeout)
        while ContinuousClock.now < deadline {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}

private actor Counter {
    private var count = 0
    func increment() { count += 1 }
    func value() -> Int { count }
}
