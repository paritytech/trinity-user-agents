import Foundation
import os

enum DataChannelSendAttemptResult {
    case sent
    case wait
    case closed
    case failed
}

struct DataChannelSendWatermarkPolicy {
    let highWatermark: UInt64
    let lowWatermark: UInt64

    init(highWatermark: UInt64, lowWatermark: UInt64) {
        precondition(lowWatermark < highWatermark)
        self.highWatermark = highWatermark
        self.lowWatermark = lowWatermark
    }

    func shouldWait(
        bufferedAmount: UInt64,
        byteCount: UInt64,
        wasThrottled: Bool
    ) -> Bool {
        // An oversized message can never fit under the high watermark, so
        // isolate it by waiting until WebRTC's existing buffer is empty.
        if byteCount > highWatermark {
            return bufferedAmount > 0
        }

        // After throttling starts, keep waiting until WebRTC drains to the low
        // watermark to avoid repeatedly pausing and resuming near the high one.
        if wasThrottled, bufferedAmount > lowWatermark {
            return true
        }

        // Before throttling, wait if WebRTC's currently buffered data plus the
        // new message would exceed the high watermark.
        let (projectedAmount, overflow) = bufferedAmount.addingReportingOverflow(byteCount)
        return overflow || projectedAmount > highWatermark
    }
}

final class DataChannelSendDrainer<Payload: Sendable>: @unchecked Sendable {
    typealias SendAttempt = @Sendable (
        _ payload: Payload,
        _ byteCount: UInt64,
        _ wasThrottled: Bool
    ) async -> DataChannelSendAttemptResult

    private let attempt: SendAttempt
    private let outbox: AsyncStream<DataChannelSendRequest<Payload>>
    private let outboxContinuation: AsyncStream<DataChannelSendRequest<Payload>>.Continuation
    private let wakeups: AsyncStream<Void>
    private let wakeupContinuation: AsyncStream<Void>.Continuation
    private let terminalError = OSAllocatedUnfairLock<AsyncDataChannelWrapperError?>(initialState: nil)

    private var drainerTask: Task<Void, Never>?

    init(attempt: @escaping SendAttempt) {
        self.attempt = attempt
        (outbox, outboxContinuation) = AsyncStream.makeStream(bufferingPolicy: .unbounded)
        (wakeups, wakeupContinuation) = AsyncStream.makeStream(bufferingPolicy: .bufferingNewest(1))

        let terminalError = terminalError
        drainerTask = Task { [outbox, wakeups, outboxContinuation, wakeupContinuation] in
            await Self.drain(
                outbox: outbox,
                wakeups: wakeups,
                attempt: attempt,
                terminalError: terminalError,
                outboxContinuation: outboxContinuation,
                wakeupContinuation: wakeupContinuation
            )
        }
    }

    deinit {
        outboxContinuation.finish()
        wakeupContinuation.yield()
        drainerTask?.cancel()
    }

    func send(_ payload: Payload, byteCount: UInt64) async throws {
        try Task.checkCancellation()
        let request = DataChannelSendRequest(payload: payload, byteCount: byteCount)

        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard request.register(continuation) else { return }

                switch outboxContinuation.yield(request) {
                case .enqueued:
                    return
                case .dropped:
                    request.complete(.failure(currentTerminalError()))
                case .terminated:
                    request.complete(.failure(currentTerminalError()))
                @unknown default:
                    request.complete(.failure(currentTerminalError()))
                }
            }
        } onCancel: {
            if request.cancel() {
                wakeupContinuation.yield()
            }
        }
    }

    func wakeUp() {
        wakeupContinuation.yield()
    }

    func terminate(with error: AsyncDataChannelWrapperError) {
        Self.terminate(
            with: error,
            terminalError: terminalError,
            outboxContinuation: outboxContinuation,
            wakeupContinuation: wakeupContinuation
        )
    }

    func waitUntilFinished() async {
        await drainerTask?.value
    }
}

private extension DataChannelSendDrainer {
    static func drain(
        outbox: AsyncStream<DataChannelSendRequest<Payload>>,
        wakeups: AsyncStream<Void>,
        attempt: @escaping SendAttempt,
        terminalError: OSAllocatedUnfairLock<AsyncDataChannelWrapperError?>,
        outboxContinuation: AsyncStream<DataChannelSendRequest<Payload>>.Continuation,
        wakeupContinuation: AsyncStream<Void>.Continuation
    ) async {
        var wakeupIterator = wakeups.makeAsyncIterator()
        var wasThrottled = false

        for await request in outbox {
            if let error = terminalError.withLock({ $0 }) {
                request.complete(.failure(error))
                continue
            }

            guard !request.isCancelled else { continue }

            requestLoop: while true {
                if let error = terminalError.withLock({ $0 }) {
                    request.complete(.failure(error))
                    break requestLoop
                }

                guard !request.isCancelled else {
                    break requestLoop
                }

                let result = await attempt(request.payload, request.byteCount, wasThrottled)

                switch result {
                case .sent:
                    wasThrottled = false
                    request.complete(.success(()))
                    break requestLoop
                case .wait:
                    wasThrottled = true
                    _ = await wakeupIterator.next()
                case .closed:
                    terminate(
                        with: .closed,
                        terminalError: terminalError,
                        outboxContinuation: outboxContinuation,
                        wakeupContinuation: wakeupContinuation
                    )
                    request.complete(.failure(.closed))
                    break requestLoop
                case .failed:
                    request.complete(.failure(.sendFailed))
                    break requestLoop
                }
            }
        }
    }

    static func terminate(
        with error: AsyncDataChannelWrapperError,
        terminalError: OSAllocatedUnfairLock<AsyncDataChannelWrapperError?>,
        outboxContinuation: AsyncStream<DataChannelSendRequest<Payload>>.Continuation,
        wakeupContinuation: AsyncStream<Void>.Continuation
    ) {
        let didTerminate = terminalError.withLock { terminalError in
            guard terminalError == nil else { return false }
            terminalError = error
            return true
        }

        guard didTerminate else { return }
        outboxContinuation.finish()
        wakeupContinuation.yield()
    }

    func currentTerminalError() -> AsyncDataChannelWrapperError {
        terminalError.withLock { $0 ?? .closed }
    }
}

private final class DataChannelSendRequest<Payload: Sendable>: @unchecked Sendable {
    private enum State {
        case awaitingRegistration
        case pending(CheckedContinuation<Void, Error>)
        case cancelled
        case completed
    }

    let payload: Payload
    let byteCount: UInt64

    private let state = OSAllocatedUnfairLock<State>(initialState: .awaitingRegistration)

    init(payload: Payload, byteCount: UInt64) {
        self.payload = payload
        self.byteCount = byteCount
    }

    var isCancelled: Bool {
        state.withLock {
            if case .cancelled = $0 { return true }
            return false
        }
    }

    func register(_ continuation: CheckedContinuation<Void, Error>) -> Bool {
        let shouldEnqueue = state.withLock { state in
            switch state {
            case .awaitingRegistration:
                state = .pending(continuation)
                return true
            case .cancelled:
                return false
            case .pending,
                 .completed:
                assertionFailure("Data channel send continuation registered more than once")
                return false
            }
        }

        if !shouldEnqueue {
            continuation.resume(throwing: CancellationError())
        }

        return shouldEnqueue
    }

    @discardableResult
    func cancel() -> Bool {
        let action = state.withLock { state -> (Bool, CheckedContinuation<Void, Error>?) in
            switch state {
            case .awaitingRegistration:
                state = .cancelled
                return (true, nil)
            case let .pending(continuation):
                state = .cancelled
                return (true, continuation)
            case .cancelled,
                 .completed:
                return (false, nil)
            }
        }

        action.1?.resume(throwing: CancellationError())

        return action.0
    }

    func complete(_ result: Result<Void, AsyncDataChannelWrapperError>) {
        let continuation = state.withLock { state -> CheckedContinuation<Void, Error>? in
            guard case let .pending(continuation) = state else { return nil }
            state = .completed
            return continuation
        }

        guard let continuation else { return }

        switch result {
        case .success:
            continuation.resume()
        case let .failure(error):
            continuation.resume(throwing: error)
        }
    }
}
