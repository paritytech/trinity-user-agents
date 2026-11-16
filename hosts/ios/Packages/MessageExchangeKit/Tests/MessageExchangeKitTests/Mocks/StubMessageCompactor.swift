@testable import MessageExchangeKit

struct StubMessageCompactor: MessageCompacting {
    func compact(messages _: [TestMessage]) async throws -> TestMessage {
        throw CancellationError()
    }
}
