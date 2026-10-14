@testable import polkadot_app
import AsyncExtensions
import Foundation

final class MockPeerConnectionSignaler: PeerConnectionSignaling {
    private let lock = NSLock()
    private let subject = AsyncPassthroughSubject<PeerConnectionSignal>()
    private var signalsSent: [PeerConnectionSignal] = []
    private var batchesSent: [[PeerConnectionSignal]] = []
    private var errors: [Error]
    private let sendAttemptStream: AsyncStream<[PeerConnectionSignal]>
    private let sendAttemptContinuation: AsyncStream<[PeerConnectionSignal]>.Continuation

    init(sendErrors: [Error] = []) {
        errors = sendErrors
        (sendAttemptStream, sendAttemptContinuation) = AsyncStream.makeStream()
    }

    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        subject.eraseToAnyAsyncSequence()
    }

    var sentSignals: [PeerConnectionSignal] {
        lock.withLock { signalsSent }
    }

    var sentBatches: [[PeerConnectionSignal]] {
        lock.withLock { batchesSent }
    }

    var sendAttempts: AsyncStream<[PeerConnectionSignal]> {
        sendAttemptStream
    }

    @discardableResult
    func send(
        _ signals: [PeerConnectionSignal]
    ) async throws -> PeerConnectionSignalSendResult {
        let error = lock.withLock {
            errors.isEmpty ? nil : errors.removeFirst()
        }

        if let error {
            sendAttemptContinuation.yield(signals)
            throw error
        }

        lock.withLock {
            signalsSent.append(contentsOf: signals)
            batchesSent.append(signals)
        }
        sendAttemptContinuation.yield(signals)

        return .fullySent
    }
}
