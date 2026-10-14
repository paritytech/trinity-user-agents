import Foundation
import StructuredConcurrency

protocol MessageDeliveryConfirming: AnyObject, Sendable {
    func awaitConfirmation(
        of messageId: Chat.MessageId,
        timeout: TimeInterval
    ) async throws
}

final class MessageDeliveryConfirmer: MessageDeliveryConfirming, @unchecked Sendable {
    private let providerFactory: ChatMessageDataProviderMaking
    private let workQueue: DispatchQueue

    init(
        providerFactory: ChatMessageDataProviderMaking = ChatMessageDataProviderFactory(),
        workQueue: DispatchQueue = DispatchQueue(
            label: "MessageDeliveryConfirmer.queue",
            qos: .utility
        )
    ) {
        self.providerFactory = providerFactory
        self.workQueue = workQueue
    }

    func awaitConfirmation(
        of messageId: Chat.MessageId,
        timeout: TimeInterval
    ) async throws {
        try await withTimeout(.seconds(timeout)) { [providerFactory, workQueue] in
            try await ChatMessageStatusAwaiter.waitUntilOutgoingStatusReached(
                messageId: messageId,
                target: .sent,
                providerFactory: providerFactory,
                workQueue: workQueue
            )
        }
    }
}
