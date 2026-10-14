@testable import polkadot_app
import Foundation

final class ManualDeviceSyncSleeper: @unchecked Sendable {
    enum Outcome: Equatable {
        case completed
        case cancelled
    }

    private let started = DeviceSyncTestEventRecorder<Void>()
    private let outcomes = DeviceSyncTestEventRecorder<Outcome>()
    private let lock = NSLock()
    private var latestRequest: ManualDeviceSyncSleepRequest?

    func sleep(for _: Duration) async throws {
        let request = ManualDeviceSyncSleepRequest()
        lock.withLock { latestRequest = request }
        started.record(())
        await withTaskCancellationHandler {
            await request.wait()
        } onCancel: {
            request.finish()
        }

        guard !Task.isCancelled else {
            outcomes.record(.cancelled)
            throw CancellationError()
        }
        outcomes.record(.completed)
    }

    func waitUntilSleeping() async {
        _ = await started.waitForCount(1)
    }

    func advance() {
        lock.withLock { latestRequest }?.finish()
    }

    func waitForOutcome() async -> Outcome {
        let outcomes = await outcomes.waitForCount(1)
        return outcomes[0]
    }
}

private final class ManualDeviceSyncSleepRequest: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?
    private var isFinished = false

    func wait() async {
        await withCheckedContinuation { continuation in
            let shouldResume = lock.withLock {
                guard !isFinished else { return true }
                self.continuation = continuation
                return false
            }
            if shouldResume { continuation.resume() }
        }
    }

    func finish() {
        let continuation = lock.withLock {
            guard !isFinished else { return nil as CheckedContinuation<Void, Never>? }
            isFinished = true
            defer { self.continuation = nil }
            return self.continuation
        }
        continuation?.resume()
    }
}
