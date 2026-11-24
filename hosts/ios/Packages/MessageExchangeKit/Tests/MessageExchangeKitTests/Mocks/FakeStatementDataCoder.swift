import Foundation
@testable import MessageExchangeKit

/// Deterministic coder used by tests to control payload sizes.
///
/// - Returns a payload of `headerSize + messages.count * messageSize` bytes
///   for any request, OR a much larger payload when any message id is in
///   `oversizedIds` (used to simulate "this single message is too big").
/// - The payload bytes themselves are arbitrary; only size matters for the
///   queue under test.
final class FakeStatementDataCoder: StatementDataCoding {
    let headerSize: Int
    let messageSize: Int
    let oversizedIds: Set<UInt32>

    init(headerSize: Int, messageSize: Int, oversizedIds: Set<UInt32> = []) {
        self.headerSize = headerSize
        self.messageSize = messageSize
        self.oversizedIds = oversizedIds
    }

    func decodeFromScaleEncodedPayload<M>(
        _: Data,
        senderAccountId _: Data?,
        route _: PeerSessionRoute
    ) throws -> StatementDataDecodingResult<M> where M: MessageExchange.CodableMessage {
        throw StatementDataDecodingError.decodingFailed
    }

    func encodeToScaleEncodedPayload(
        _ statementData: StatementData<some MessageExchange.CodableMessage>,
        route _: PeerSessionRoute
    ) throws -> Data {
        guard case let .request(request) = statementData else {
            return Data(count: headerSize)
        }

        if let testMessages = request.messages as? [TestMessage],
           testMessages.contains(where: { oversizedIds.contains($0.id) }) {
            return Data(count: 10_000)
        }

        return Data(count: headerSize + request.messages.count * messageSize)
    }
}
