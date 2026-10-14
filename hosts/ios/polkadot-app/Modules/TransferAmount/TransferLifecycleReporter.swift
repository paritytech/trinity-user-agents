import AsyncExtensions
import Coinage
import Foundation
import SubstrateSdk

/// Context handed to a lifecycle reporter once the transfer has been submitted.
enum TransferTrackingContext {
    case coinageMemo(TransferMemo)
    case externalPayment(productId: String, paymentId: String, amountInPlanks: Balance)
}

/// Streams post-submission transfer lifecycle updates to the UI.
///
/// Contract: ``makeStream()`` always terminates after the last meaningful state.
/// A graceful finish whose last status is not `.failed` means the transfer succeeded.
/// ``start(with:)`` is non-blocking — tracking must not delay or fail the submission path.
protocol TransferLifecycleReporting: Sendable {
    func makeStream() -> AnyAsyncSequence<OutgoingTransferState>
    func start(with context: TransferTrackingContext)
}

/// Used for flows without post-submission tracking: success is reported
/// as soon as submission completes because the stream finishes immediately.
struct NoOpTransferLifecycleReporter: TransferLifecycleReporting {
    func makeStream() -> AnyAsyncSequence<OutgoingTransferState> {
        AsyncStream<OutgoingTransferState> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func start(with _: TransferTrackingContext) {}
}

/// Surfaced by the presenter when the lifecycle stream ends in a failed state.
enum TransferLifecycleError: Error {
    case transferFailed
}
