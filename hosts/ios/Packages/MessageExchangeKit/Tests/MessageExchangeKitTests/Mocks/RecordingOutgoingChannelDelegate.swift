import Foundation_iOS
@testable import MessageExchangeKit

final class RecordingOutgoingChannelDelegate: OutgoingMessageChannelDelegate, TypeErasedDelegateStoring {
    private(set) var addToQueueCalls: [(message: TestMessage, error: MessageExchange.AddToQueueError?)] = []

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didFinishAddingMessageToQueue message: TestMessage,
        withError error: MessageExchange.AddToQueueError?
    ) {
        addToQueueCalls.append((message, error))
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didPostMessages _: [TestMessage],
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didDeliverMessages _: [TestMessage],
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didCompactMessages _: TestMessage,
        originalMessages _: [TestMessage]
    ) {}

    func statementSubmitFailed(with _: Error) {}
}
