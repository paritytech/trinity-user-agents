import Foundation
import Testing
@testable import MessageExchangeKit

struct OutgoingRequestQueueCoalescingTests {
    private let coalescer = AnyMessageCoalescer(
        StubMessageCoalescer(coalescableIds: [100, 101], fixedDecision: .ignore)
    )
    private let replacingCoalescer = AnyMessageCoalescer(
        StubMessageCoalescer(coalescableIds: [100, 101], fixedDecision: .replace)
    )

    @Test func identicalReceiptIsIgnoredAgainstCurrentRequest() {
        let queue = makeQueue(coalescer: coalescer)
        let currentRequest = makeRequest(messages: [TestMessage(id: 1), TestMessage(id: 100)])
        queue.currentRequests[.device] = currentRequest

        let results = queue.addMessages([TestMessage(id: 101)], isChannelActive: true)

        #expect(successValue(results[0]) == .ignored)
        #expect(queue.currentRequests[.device]?.requestId == currentRequest.requestId)
        #expect(queue.currentRequests[.device]?.messages.map(\.id) == [1, 100])
    }

    @Test func identicalReceiptIsIgnoredAgainstQueuedMessages() {
        let queue = makeQueue(coalescer: coalescer)

        let results = queue.addMessages(
            [TestMessage(id: 100), TestMessage(id: 101)],
            isChannelActive: false
        )

        #expect(results.map { successValue($0) } == [.queued, .ignored])
        #expect(queue.dequeueMessagesForNewRequest()?.messages.map(\.id) == [100])
    }

    @Test func restoredRequestSuppressesQueuedReceiptOnActivate() {
        let queue = makeQueue(coalescer: coalescer)
        let queuedResults = queue.addMessages([TestMessage(id: 101)], isChannelActive: false)
        #expect(successValue(queuedResults[0]) == .queued)

        let restoredRequest = makeRequest(messages: [TestMessage(id: 100)])
        queue.currentRequests[.device] = restoredRequest

        let extendedRoutes = queue.attemptRequestExtensionFromQueue()

        #expect(extendedRoutes.isEmpty)
        #expect(queue.currentRequests[.device]?.requestId == restoredRequest.requestId)
        #expect(queue.currentRequests[.device]?.messages.map(\.id) == [100])
        #expect(queue.dequeueMessagesForNewRequest() == nil)
    }

    @Test func defaultPolicyAppendsIdenticalReceipt() {
        let queue = makeQueue()
        queue.currentRequests[.device] = makeRequest(messages: [TestMessage(id: 100)])

        let results = queue.addMessages([TestMessage(id: 101)], isChannelActive: true)

        #expect(successValue(results[0]) == .appendedToCurrentRequest(route: .device))
    }

    @Test func newerReceiptReplacesPendingOneInCurrentRequest() {
        let queue = makeQueue(coalescer: replacingCoalescer)
        let currentRequest = makeRequest(messages: [TestMessage(id: 1), TestMessage(id: 100)])
        queue.currentRequests[.device] = currentRequest

        let results = queue.addMessages([TestMessage(id: 101)], isChannelActive: true)

        #expect(successValue(results[0]) == .appendedToCurrentRequest(route: .device))
        #expect(queue.currentRequests[.device]?.messages.map(\.id) == [1, 101])
        #expect(queue.currentRequests[.device]?.requestId != currentRequest.requestId)
    }

    @Test func newerReceiptReplacesPendingOneInQueue() {
        let queue = makeQueue(coalescer: replacingCoalescer)

        let results = queue.addMessages(
            [TestMessage(id: 100), TestMessage(id: 101)],
            isChannelActive: false
        )

        #expect(results.map { successValue($0) } == [.queued, .queued])
        #expect(queue.dequeueMessagesForNewRequest()?.messages.map(\.id) == [101])
    }

    @Test func restoredRequestReceiptIsReplacedOnActivate() {
        let queue = makeQueue(coalescer: replacingCoalescer)
        let queuedResults = queue.addMessages([TestMessage(id: 101)], isChannelActive: false)
        #expect(successValue(queuedResults[0]) == .queued)

        let restoredRequest = makeRequest(messages: [TestMessage(id: 100)])
        queue.currentRequests[.device] = restoredRequest

        let extendedRoutes = queue.attemptRequestExtensionFromQueue()

        #expect(extendedRoutes == [.device])
        #expect(queue.currentRequests[.device]?.messages.map(\.id) == [101])
        #expect(queue.currentRequests[.device]?.requestId != restoredRequest.requestId)
        #expect(queue.dequeueMessagesForNewRequest() == nil)
    }
}

private extension OutgoingRequestQueueCoalescingTests {
    func makeQueue(
        coalescer: AnyMessageCoalescer<TestMessage> = .none
    ) -> OutgoingRequestQueue<TestMessage> {
        OutgoingRequestQueue(
            statementDataCoder: FakeStatementDataCoder(headerSize: 10, messageSize: 10),
            routeSelector: PeerSessionRouteSelector { $0.route },
            sizeValidator: FixedSizeValidator(maxPayloadSize: 1_000),
            supportedRoutes: [.device],
            logger: nil,
            coalescer: coalescer
        )
    }

    func makeRequest(messages: [TestMessage]) -> OutgoingRequest<TestMessage> {
        OutgoingRequest(
            requestId: UUID().uuidString,
            messages: messages,
            scaleEncodedPayload: Data(),
            route: .device
        )
    }

    func successValue(
        _ result: Result<MessageExchange.AddToQueueResult, MessageExchange.AddToQueueError>
    ) -> MessageExchange.AddToQueueResult? {
        try? result.get()
    }
}
