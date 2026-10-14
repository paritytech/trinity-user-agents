import Foundation

protocol IncomingMessageProcessing {
    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact)
}

final class CompoundIncomingMessageProcessor: IncomingMessageProcessing {
    let processors: [IncomingMessageProcessing]

    init(processors: [IncomingMessageProcessing]) {
        self.processors = processors
    }

    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact) {
        processors.forEach { $0.process(messages: messages, from: contact) }
    }
}
