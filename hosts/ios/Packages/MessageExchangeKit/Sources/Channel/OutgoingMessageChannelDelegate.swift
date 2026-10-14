import Foundation
import Foundation_iOS

protocol OutgoingMessageChannelDelegate: MessageChannelDelegate {
    associatedtype Message: MessageExchange.CodableMessage

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    )

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    )

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    )

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    )
}

// MARK: - Type Erasure Implementation

final class AnyOutgoingMessageChannelDelegate<M: MessageExchange.CodableMessage>: OutgoingMessageChannelDelegate {
    typealias Message = M

    private let didFinishAddingMessageToQueueClosure: (
        any OutgoingMessageChanneling,
        Message,
        MessageExchange.AddToQueueError?
    ) -> Void

    private let didPostMessagesClosure: (
        any OutgoingMessageChanneling,
        [Message],
        MessageExchange.OutgoingMessageError?
    ) -> Void

    private let didDeliverMessagesClosure: (
        any OutgoingMessageChanneling,
        [Message],
        MessageExchange.OutgoingMessageError?
    ) -> Void

    private let didCompactMessagesClosure: (
        any OutgoingMessageChanneling,
        Message,
        [Message]
    ) -> Void

    private let statementSubmitFailedClosure: (Error) -> Void

    init<
        D: OutgoingMessageChannelDelegate & TypeErasedDelegateStoring
    >(_ targetDelegate: D) where D.Message == M {
        didFinishAddingMessageToQueueClosure = { [weak targetDelegate] channel, message, error in
            targetDelegate?.messageChannel(
                channel,
                didFinishAddingMessageToQueue: message,
                withError: error
            )
        }

        didPostMessagesClosure = { [weak targetDelegate] channel, messages, error in
            targetDelegate?.messageChannel(
                channel,
                didPostMessages: messages,
                withError: error
            )
        }

        didDeliverMessagesClosure = { [weak targetDelegate] channel, messages, error in
            targetDelegate?.messageChannel(
                channel,
                didDeliverMessages: messages,
                withError: error
            )
        }

        didCompactMessagesClosure = { [weak targetDelegate] channel, compactedMessage, originalMessages in
            targetDelegate?.messageChannel(
                channel,
                didCompactMessages: compactedMessage,
                originalMessages: originalMessages
            )
        }

        statementSubmitFailedClosure = { [weak targetDelegate] error in
            targetDelegate?.statementSubmitFailed(with: error)
        }

        targetDelegate.storeErasedType(instance: self)
    }

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    ) {
        didFinishAddingMessageToQueueClosure(channel, message, error)
    }

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didPostMessagesClosure(channel, messages, error)
    }

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didDeliverMessagesClosure(channel, messages, error)
    }

    func messageChannel(
        _ channel: any OutgoingMessageChanneling,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    ) {
        didCompactMessagesClosure(channel, compactedMessage, originalMessages)
    }

    func statementSubmitFailed(with error: Error) {
        statementSubmitFailedClosure(error)
    }
}
