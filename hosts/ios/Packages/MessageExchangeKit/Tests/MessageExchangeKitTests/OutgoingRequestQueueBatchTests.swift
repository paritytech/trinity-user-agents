import XCTest
import SubstrateSdk
@testable import MessageExchangeKit

final class OutgoingRequestQueueBatchTests: XCTestCase {
    func testBatchAllFitInCurrentRequest() {
        let queue = makeQueue(maxPayloadSize: 1_000)
        queue.currentRequests[.device] = makeRequest(messages: [TestMessage(id: 0)])

        let results = queue.addMessages(
            [TestMessage(id: 1), TestMessage(id: 2)],
            isChannelActive: true
        )

        XCTAssertEqual(results.count, 2)
        XCTAssertEqual(
            results.compactMap(successValue),
            [.appendedToCurrentRequest(route: .device), .appendedToCurrentRequest(route: .device)]
        )
        XCTAssertEqual(queue.currentRequests[.device]?.messages.map(\.id), [0, 1, 2])
    }

    func testBatchPartialSpillToBackingQueue() {
        // FakeCoder reports `messageSize * N + headerSize` bytes for N messages.
        // header=10, message=10, max=30 → 1 base message + 1 appended fits (30B), 2 appended overflows (40B).
        let queue = makeQueue(
            statementDataCoder: FakeStatementDataCoder(headerSize: 10, messageSize: 10),
            maxPayloadSize: 30
        )
        queue.currentRequests[.device] = makeRequest(
            statementDataCoder: FakeStatementDataCoder(headerSize: 10, messageSize: 10),
            messages: [TestMessage(id: 0)]
        )

        let results = queue.addMessages(
            [TestMessage(id: 1), TestMessage(id: 2), TestMessage(id: 3)],
            isChannelActive: true
        )

        XCTAssertEqual(results.count, 3)
        XCTAssertEqual(successValue(results[0]), .appendedToCurrentRequest(route: .device))
        XCTAssertEqual(successValue(results[1]), .queued)
        XCTAssertEqual(successValue(results[2]), .queued)

        XCTAssertEqual(queue.currentRequests[.device]?.messages.map(\.id), [0, 1])
        XCTAssertNil(queue.dequeueMessagesForNewRequest())

        queue.currentRequests[.device] = nil
        let dequeued = queue.dequeueMessagesForNewRequest()
        XCTAssertEqual(dequeued?.messages.map(\.id), [2, 3])
    }

    func testBatchOversizedMessageInMiddle() {
        // Single-message payload (header + 1 message = 20B) fits within max=25,
        // but the oversized middle message reports 100B alone and is rejected.
        let coder = FakeStatementDataCoder(headerSize: 10, messageSize: 10, oversizedIds: [42])
        let queue = makeQueue(statementDataCoder: coder, maxPayloadSize: 25)

        let results = queue.addMessages(
            [TestMessage(id: 1), TestMessage(id: 42), TestMessage(id: 3)],
            isChannelActive: true
        )

        XCTAssertEqual(results.count, 3)
        XCTAssertEqual(successValue(results[0]), .queued)

        if case let .failure(error) = results[1] {
            guard case .messageTooBig = error else {
                XCTFail("Expected .messageTooBig for oversized middle message, got \(error)")
                return
            }
        } else {
            XCTFail("Expected failure for oversized middle message")
        }

        XCTAssertEqual(successValue(results[2]), .queued, "Trailing message should still enqueue after oversized one")
    }

    func testBatchDuplicateInsideBatchIsIgnored() {
        let queue = makeQueue(maxPayloadSize: 1_000)
        queue.currentRequests[.device] = makeRequest(messages: [TestMessage(id: 0)])

        let dup = TestMessage(id: 1)
        let results = queue.addMessages([dup, dup], isChannelActive: true)

        XCTAssertEqual(results.count, 2)
        XCTAssertEqual(successValue(results[0]), .appendedToCurrentRequest(route: .device))
        XCTAssertEqual(successValue(results[1]), .ignored)
    }

    func testDifferentRouteDoesNotWaitBehindCurrentDefaultRequest() {
        let queue = makeQueue(maxPayloadSize: 1_000)
        queue.currentRequests[.device] = makeRequest(messages: [TestMessage(id: 0)])

        let results = queue.addMessages(
            [TestMessage(id: 1, route: .identity)],
            isChannelActive: true
        )

        XCTAssertEqual(successValue(results[0]), .queued)

        let dequeued = queue.dequeueMessagesForNewRequest()

        XCTAssertEqual(dequeued?.route, .identity)
        XCTAssertEqual(dequeued?.messages.map(\.id), [1])
    }

    func testCurrentRequestsAreExtendedByRoute() {
        let queue = makeQueue(maxPayloadSize: 1_000)
        queue.currentRequests[.device] = makeRequest(messages: [TestMessage(id: 0)])
        queue.currentRequests[.identity] = makeRequest(messages: [TestMessage(id: 10, route: .identity)])

        let results = queue.addMessages(
            [
                TestMessage(id: 1),
                TestMessage(id: 11, route: .identity)
            ],
            isChannelActive: true
        )

        XCTAssertEqual(
            results.compactMap(successValue),
            [.appendedToCurrentRequest(route: .device), .appendedToCurrentRequest(route: .identity)]
        )
        XCTAssertEqual(queue.currentRequests[.device]?.messages.map(\.id), [0, 1])
        XCTAssertEqual(queue.currentRequests[.identity]?.messages.map(\.id), [10, 11])
    }

    func testUnsupportedRouteFailsAdd() {
        let queue = makeQueue(maxPayloadSize: 1_000, supportedRoutes: [.device])

        let results = queue.addMessages(
            [TestMessage(id: 1, route: .identity)],
            isChannelActive: true
        )

        if case let .failure(error) = results[0] {
            guard case .unsupportedRoute(.identity) = error else {
                XCTFail("Expected .unsupportedRoute(.identity), got \(error)")
                return
            }
        } else {
            XCTFail("Expected unsupported route failure")
        }

        XCTAssertNil(queue.dequeueMessagesForNewRequest())
    }
}

// MARK: - Helpers

private extension OutgoingRequestQueueBatchTests {
    func makeQueue(
        statementDataCoder: StatementDataCoding = FakeStatementDataCoder(headerSize: 10, messageSize: 10),
        maxPayloadSize: Int,
        supportedRoutes: [PeerSessionRoute] = [.device, .identity]
    ) -> OutgoingRequestQueue<TestMessage> {
        OutgoingRequestQueue(
            statementDataCoder: statementDataCoder,
            routeSelector: PeerSessionRouteSelector { $0.route },
            sizeValidator: FixedSizeValidator(maxPayloadSize: maxPayloadSize),
            supportedRoutes: supportedRoutes,
            logger: nil
        )
    }

    func makeRequest(
        statementDataCoder: StatementDataCoding = FakeStatementDataCoder(headerSize: 10, messageSize: 10),
        messages: [TestMessage]
    ) -> OutgoingRequest<TestMessage> {
        let request = MessageExchange.Request(requestId: "seed", messages: messages)
        // swiftlint:disable:next force_try
        let route = messages.first?.route ?? .device
        let payload = try! statementDataCoder.encodeToScaleEncodedPayload(.request(request), route: route)
        return OutgoingRequest(
            requestId: request.requestId,
            messages: messages,
            scaleEncodedPayload: payload,
            route: route
        )
    }

    func successValue(
        _ result: Result<MessageExchange.AddToQueueResult, MessageExchange.AddToQueueError>
    ) -> MessageExchange.AddToQueueResult? {
        if case let .success(value) = result { return value }
        return nil
    }
}

// MARK: - Fixtures

struct TestMessage: MessageExchange.CodableMessage {
    let id: UInt32
    let route: PeerSessionRoute

    init(
        id: UInt32,
        route: PeerSessionRoute = .device
    ) {
        self.id = id
        self.route = route
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        id = try UInt32(scaleDecoder: scaleDecoder)
        route = .device
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
    }
}

private final class FixedSizeValidator: OutgoingRequestSizeValidating {
    let maxPayloadSize: Int

    init(maxPayloadSize: Int) {
        self.maxPayloadSize = maxPayloadSize
    }
}

/// Deterministic coder used by tests to control payload sizes.
///
/// - Returns a payload of `headerSize + messages.count * messageSize` bytes
///   for any request, OR a much larger payload when any message id is in
///   `oversizedIds` (used to simulate "this single message is too big").
/// - The payload bytes themselves are arbitrary; only size matters for the
///   queue under test.
private final class FakeStatementDataCoder: StatementDataCoding {
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
