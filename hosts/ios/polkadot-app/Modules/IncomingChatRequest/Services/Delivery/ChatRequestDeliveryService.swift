import BackgroundExecution
import Foundation
import MessageExchangeKit

struct ChatRequestDeliveryExecution {
    let backgroundExecutor: BackgroundExecuting
    var clock: any Clock<Duration> = ContinuousClock()
}

protocol ChatRequestDelivering: Sendable {
    /// Delivers a recorded outgoing request, retrying with backoff for as long as it still awaits delivery.
    func deliverUntilDone(_ message: Chat.RequestMessage, session: MessageExchange.SessionRequest) async
}

/// First delivery of recorded outgoing chat requests, signed by the account the resolver picks.
final class ChatRequestDeliveryService: @unchecked Sendable {
    private static let firstRetryDelay: Duration = .seconds(2)
    private static let maxRetryDelay: Duration = .seconds(5 * 60)
    private static let maxBackoffDoublings = 8

    private let outgoingService: OutgoingChatRequestServicing
    private let resolver: ChatRequestDeliveryAccountResolving
    private let store: ChatRequestDeliveryStoring
    private let execution: ChatRequestDeliveryExecution
    private let diagnostics: ChatRequestDiagnostics

    init(
        outgoingService: OutgoingChatRequestServicing,
        resolver: ChatRequestDeliveryAccountResolving,
        store: ChatRequestDeliveryStoring,
        execution: ChatRequestDeliveryExecution,
        diagnostics: ChatRequestDiagnostics
    ) {
        self.outgoingService = outgoingService
        self.resolver = resolver
        self.store = store
        self.execution = execution
        self.diagnostics = diagnostics
    }
}

extension ChatRequestDeliveryService: ChatRequestDelivering {
    func deliverUntilDone(_ message: Chat.RequestMessage, session: MessageExchange.SessionRequest) async {
        var attempt = 0

        while await isAwaitingDelivery(message.messageId) {
            do {
                try await execution.backgroundExecutor.execute { try await self.deliver(message, session: session) }
                diagnostics.issues.deliveryStalled.recordRecovery(for: message.messageId)
                diagnostics.logger.debug("Chat request \(message.messageId) finished after \(attempt + 1) attempt(s)")
                return
            } catch {
                attempt += 1
                let retryIn = Self.retryDelay(forAttempt: attempt)
                diagnostics.logger.error("Chat request \(message.messageId) attempt \(attempt) failed: \(error)")
                diagnostics.issues.deliveryStalled.recordFailure(for: message.messageId, error: error, counters: [:])

                do {
                    try await execution.clock.sleep(for: retryIn)
                } catch {
                    return
                }
            }
        }
    }
}

private extension ChatRequestDeliveryService {
    func deliver(_ message: Chat.RequestMessage, session: MessageExchange.SessionRequest) async throws {
        let size = try outgoingService.encodedSize(of: message, to: session.peer, ownKeyId: session.own)

        guard try await size <= resolver.maxStatementSize() else {
            diagnostics.logger.error("Chat request \(message.messageId) is \(size) bytes; marking it undeliverable")
            try await store.markFailed(requestId: message.messageId)
            diagnostics.issues.oversizedRequest.recordFailure(
                for: message.messageId,
                error: nil,
                counters: ["size": size]
            )
            return
        }

        let signer = try await resolver.resolveFirstDelivery(requestId: message.messageId)

        try await outgoingService.send(message: message, to: session.peer, ownKeyId: session.own, signer: signer.signer)
        try await store.markDelivered(requestId: message.messageId, anonymousPeriod: signer.period)
    }

    func isAwaitingDelivery(_ requestId: String) async -> Bool {
        guard !Task.isCancelled else { return false }

        do {
            return try await store.isAwaitingDelivery(requestId: requestId)
        } catch {
            diagnostics.logger.error("Chat request \(requestId) delivery state unreadable: \(error)")
            diagnostics.issues.deliveryStateUnreadable.recordFailure(for: requestId, error: error, counters: [:])
            return false
        }
    }

    static func retryDelay(forAttempt attempt: Int) -> Duration {
        let doublings = min(attempt, maxBackoffDoublings)
        return min(firstRetryDelay * (1 << doublings), maxRetryDelay)
    }
}
