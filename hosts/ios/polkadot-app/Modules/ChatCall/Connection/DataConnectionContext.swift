import Foundation
import AsyncExtensions

actor DataConnectionContext {
    typealias Sleep = @Sendable (TimeInterval) async throws -> Void

    enum Constants {
        static let defaultFlushDelay: TimeInterval = 0.5
        static let defaultLimitPerFlush = 4
    }

    struct FlushParams {
        let delayInSec: TimeInterval
        let limitPerFlush: Int
    }

    let signaler: PeerConnectionSignaling
    let logger: LoggerProtocol
    private let sleep: Sleep

    private var autoflushParams: FlushParams?

    private var signalsBuffer: [PeerConnectionSignal] = []
    private var flushTask: Task<Void, Never>?
    private var isStopped = false

    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        get async { await signaler.signals }
    }

    init(
        signaler: PeerConnectionSignaling,
        logger: LoggerProtocol,
        sleep: @escaping Sleep = { try await Task.sleep(for: .seconds($0)) }
    ) {
        self.signaler = signaler
        self.logger = logger
        self.sleep = sleep
    }

    func append(_ signal: PeerConnectionSignal) {
        guard !isStopped else { return }

        signalsBuffer.append(signal)

        guard let autoflushParams else { return }

        ensureFlushTaskRunning(with: autoflushParams)
    }

    func sendSignalAndFlushBuffer(
        _ signal: PeerConnectionSignal,
        flushDelay: TimeInterval = Constants.defaultFlushDelay,
        limitPerFlush: Int = Constants.defaultLimitPerFlush
    ) {
        guard !isStopped else { return }

        signalsBuffer.insert(signal, at: 0)

        autoflushParams = nil
        cancelFlushTask()

        ensureFlushTaskRunning(with: FlushParams(delayInSec: flushDelay, limitPerFlush: limitPerFlush))
    }

    func startAutoflush(
        with delay: TimeInterval = Constants.defaultFlushDelay,
        limitPerFlush: Int = Constants.defaultLimitPerFlush
    ) {
        guard !isStopped else { return }

        let params = FlushParams(delayInSec: delay, limitPerFlush: limitPerFlush)
        autoflushParams = params

        guard !signalsBuffer.isEmpty else {
            return
        }

        ensureFlushTaskRunning(with: params)
    }

    func stopBuffering() {
        isStopped = true
        autoflushParams = nil
        signalsBuffer.removeAll()
        cancelFlushTask()
    }
}

extension DataConnectionContext {
    func ensureFlushTaskRunning(with params: FlushParams) {
        guard flushTask == nil, !isStopped else {
            return
        }

        let sleep = sleep
        flushTask = Task { [weak self] in
            try? await sleep(params.delayInSec)
            await self?.performFlushBuffer(with: params.limitPerFlush)
        }
    }

    func cancelFlushTask() {
        flushTask?.cancel()
        flushTask = nil
    }

    func performFlushBuffer(with limit: Int) async {
        guard !Task.isCancelled else { return }

        guard !isStopped, !signalsBuffer.isEmpty else {
            flushTask = nil
            return
        }

        let itemsToSend = Array(signalsBuffer.prefix(limit))
        signalsBuffer = Array(signalsBuffer.dropFirst(limit))

        do {
            try await send(itemsToSend)
            try Task.checkCancellation()
        } catch {
            // Intentional task cancellation is cleaned up by cancelFlushTask().
            // Downstream send failures still need to unblock autoflush.
            guard !Task.isCancelled else { return }
            finishFlushTaskAndRestartIfNeeded()
            return
        }

        finishFlushTaskAndRestartIfNeeded()
    }

    func send(_ signals: [PeerConnectionSignal]) async throws {
        do {
            try Task.checkCancellation()
            try await signaler.send(signals)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.error("Failed to send signaling batch: \(error)")
        }
    }

    func finishFlushTaskAndRestartIfNeeded() {
        flushTask = nil

        guard !signalsBuffer.isEmpty, let autoflushParams, !isStopped else {
            return
        }

        ensureFlushTaskRunning(with: autoflushParams)
    }
}
