@testable import polkadot_app
import Foundation
import Testing

@Suite(.timeLimit(.minutes(1)))
struct DataChannelSendDrainerTests {
    @Test("Watermark policy applies hysteresis and isolates oversized messages")
    func watermarkHysteresis() {
        let policy = DataChannelSendWatermarkPolicy(highWatermark: 100, lowWatermark: 40)

        #expect(!policy.shouldWait(bufferedAmount: 80, byteCount: 20, wasThrottled: false))
        #expect(policy.shouldWait(bufferedAmount: 81, byteCount: 20, wasThrottled: false))
        #expect(policy.shouldWait(bufferedAmount: 41, byteCount: 20, wasThrottled: true))
        #expect(!policy.shouldWait(bufferedAmount: 40, byteCount: 20, wasThrottled: true))
        #expect(policy.shouldWait(bufferedAmount: 1, byteCount: 101, wasThrottled: false))
        #expect(!policy.shouldWait(bufferedAmount: 0, byteCount: 101, wasThrottled: true))
    }

    @Test("Send completes after the atomic attempt succeeds")
    func successfulSend() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let sendTask = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        await attempts.resolveNext(with: .sent)

        try await sendTask.value
        #expect(await attempts.calls == [.init(payload: 1, byteCount: 10, wasThrottled: false)])

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }

    @Test("A wakeup delivered before waiting is retained")
    func wakeupBeforeWaitIsRetained() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let sendTask = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        drainer.wakeUp()
        await attempts.resolveNext(with: .wait)

        await attempts.waitForCallCount(2)
        await attempts.resolveNext(with: .sent)
        try await sendTask.value

        #expect(await attempts.calls.map(\.wasThrottled) == [false, true])

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }

    @Test("Cancelling a throttled head resumes it and advances the queue")
    func cancellingThrottledHeadAdvancesQueue() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let first = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        await attempts.resolveNext(with: .wait)
        await Task.yield()

        let second = Task { try await drainer.send(2, byteCount: 20) }
        await Task.yield()
        first.cancel()

        await #expect(throws: CancellationError.self) {
            try await first.value
        }

        await attempts.waitForCallCount(2)
        await attempts.resolveNext(with: .sent)
        try await second.value

        #expect(await attempts.calls.map(\.payload) == [1, 2])
        #expect(await attempts.calls.map(\.wasThrottled) == [false, true])

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }

    @Test("FIFO is preserved across throttle and unthrottle")
    func fifoAcrossThrottleCycle() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let first = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        let second = Task { try await drainer.send(2, byteCount: 20) }
        await Task.yield()

        await attempts.resolveNext(with: .wait)
        drainer.wakeUp()
        await attempts.waitForCallCount(2)
        await attempts.resolveNext(with: .sent)

        await attempts.waitForCallCount(3)
        await attempts.resolveNext(with: .sent)

        try await first.value
        try await second.value
        #expect(await attempts.calls.map(\.payload) == [1, 1, 2])
        #expect(await attempts.calls.map(\.wasThrottled) == [false, true, false])

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }

    @Test("Close wakes a throttled request and drains the outbox")
    func closeWakesThrottledRequest() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let first = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        await attempts.resolveNext(with: .wait)
        await Task.yield()

        let second = Task { try await drainer.send(2, byteCount: 20) }
        await Task.yield()
        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()

        await #expect(throws: AsyncDataChannelWrapperError.self) {
            try await first.value
        }
        await #expect(throws: AsyncDataChannelWrapperError.self) {
            try await second.value
        }
    }

    @Test("A send failure affects only its request")
    func sendFailureDoesNotStopLaterSends() async throws {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let first = Task { try await drainer.send(1, byteCount: 10) }

        await attempts.waitForCallCount(1)
        let second = Task { try await drainer.send(2, byteCount: 20) }
        await Task.yield()
        await attempts.resolveNext(with: .failed)

        await #expect(throws: AsyncDataChannelWrapperError.self) {
            try await first.value
        }

        await attempts.waitForCallCount(2)
        await attempts.resolveNext(with: .sent)
        try await second.value

        let third = Task { try await drainer.send(3, byteCount: 30) }
        await attempts.waitForCallCount(3)
        await attempts.resolveNext(with: .sent)
        try await third.value

        #expect(await attempts.calls.map(\.payload) == [1, 2, 3])
        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }

    @Test("Enqueue after close fails immediately without an attempt")
    func enqueueAfterCloseFailsImmediately() async {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()

        await #expect(throws: AsyncDataChannelWrapperError.self) {
            try await drainer.send(1, byteCount: 10)
        }
        #expect(await attempts.calls.isEmpty)
    }

    @Test("A task cancelled before send never enters the outbox")
    func cancellationBeforeSend() async {
        let attempts = ManualDataChannelSendAttempts<Int>()
        let drainer = makeDrainer(attempts: attempts)
        let gate = DataChannelSendTestGate()
        let task = Task {
            await gate.wait()
            try await drainer.send(1, byteCount: 10)
        }

        task.cancel()
        await gate.open()

        await #expect(throws: CancellationError.self) {
            try await task.value
        }
        #expect(await attempts.calls.isEmpty)

        drainer.terminate(with: .closed)
        await drainer.waitUntilFinished()
    }
}

private extension DataChannelSendDrainerTests {
    func makeDrainer(
        attempts: ManualDataChannelSendAttempts<Int>
    ) -> DataChannelSendDrainer<Int> {
        DataChannelSendDrainer(
            attempt: { payload, byteCount, wasThrottled in
                await attempts.attempt(
                    payload: payload,
                    byteCount: byteCount,
                    wasThrottled: wasThrottled
                )
            }
        )
    }
}
