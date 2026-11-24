@testable import MessageExchangeKit

final class RecordingMessageCompactor: MessageCompacting, @unchecked Sendable {
    private(set) var compactedMessageBatches: [[TestMessage]] = []

    private let compactedMessage: TestMessage

    init(compactedMessage: TestMessage) {
        self.compactedMessage = compactedMessage
    }

    func compact(messages: [TestMessage]) async throws -> TestMessage {
        compactedMessageBatches.append(messages)
        return compactedMessage
    }
}
