import AsyncExtensions
import Coinage
import Foundation

/// Streams the lifecycle of a single W3S payment by observing its persisted record.
///
/// The record is the single source of truth: ``W3sPaymentTrackingService`` advances
/// the persisted status while this reporter only projects it into ``OutgoingTransferState``
/// for the transfer screen. `start(with:)` is a no-op — tracking is driven by the
/// app-level service, not by the screen.
final class W3sPaymentLifecycleReporter: TransferLifecycleReporting {
    private let historyStore: W3sPaymentHistoryStoring
    private let paymentId: String

    init(historyStore: W3sPaymentHistoryStoring, paymentId: String) {
        self.historyStore = historyStore
        self.paymentId = paymentId
    }

    func makeStream() -> AnyAsyncSequence<OutgoingTransferState> {
        let source = historyStore.observeRecord(paymentId: paymentId)

        return AsyncStream<OutgoingTransferState> { continuation in
            let task = Task {
                var lastStatus: OutgoingTransferState?
                do {
                    for try await record in source {
                        guard let status = record?.transferState, status != lastStatus else {
                            continue
                        }
                        lastStatus = status
                        continuation.yield(status)
                        if status.isTerminal {
                            break
                        }
                    }
                } catch {}
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    func start(with _: TransferTrackingContext) {}
}

private extension W3sPaymentRecord {
    var transferState: OutgoingTransferState {
        switch status {
        case .pending,
             .submitted:
            OutgoingTransferState(status: .sending)
        case .sent:
            OutgoingTransferState(status: .sent)
        case .claimed:
            OutgoingTransferState(status: .claimed, actualValue: memo.totalValue)
        case .failed,
             .revoked:
            OutgoingTransferState(status: .failed)
        }
    }
}
