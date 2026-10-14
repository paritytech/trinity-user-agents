import AsyncExtensions
import Coinage
import Foundation
@preconcurrency import SDKLogger

/// Projects the coinage external-payment status stream into ``OutgoingTransferState``
/// for the transfer screen. Tracking runs in an owned task so submission
/// returns immediately and the screen reacts to streamed statuses.
final class ExternalPaymentLifecycleReporter: TransferLifecycleReporting {
    private let coinageService: CoinageServicing
    private let logger: SDKLoggerProtocol?
    private let subject = AsyncCurrentValueSubject<OutgoingTransferState>(.init(status: .sending))

    init(
        coinageService: CoinageServicing,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.coinageService = coinageService
        self.logger = logger
    }

    func makeStream() -> AnyAsyncSequence<OutgoingTransferState> {
        subject.eraseToAnyAsyncSequence()
    }

    func start(with context: TransferTrackingContext) {
        guard case let .externalPayment(productId, paymentId, amountInPlanks) = context else {
            subject.send(Termination<Never>.finished)
            return
        }

        Task { [coinageService, subject, logger] in
            do {
                let statuses = coinageService.subscribeExternalPaymentStatus(
                    productId: productId,
                    paymentId: paymentId
                )

                for try await status in statuses {
                    switch status {
                    case .processing:
                        subject.send(.init(status: .sending))
                    case .completed:
                        subject.send(.init(status: .claimed, actualValue: amountInPlanks))
                        subject.send(Termination<Never>.finished)
                        return
                    case let .partiallyCompleted(settledInPlanks):
                        logger?.error("External payment \(paymentId) short: \(settledInPlanks) of \(amountInPlanks)")
                        subject.send(.init(status: .claimed, actualValue: settledInPlanks))
                        subject.send(Termination<Never>.finished)
                        return
                    case let .failed(reason):
                        logger?.error("External payment \(paymentId) failed: \(reason)")
                        subject.send(.init(status: .failed))
                        subject.send(Termination<Never>.finished)
                        return
                    }
                }

                subject.send(Termination<Never>.finished)
            } catch {
                logger?.error("External payment \(paymentId) status stream failed: \(error)")
                subject.send(.init(status: .failed))
                subject.send(Termination<Never>.finished)
            }
        }
    }
}
