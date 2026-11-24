import Foundation
import Testing
@testable import MessageExchangeKit

struct OutgoingChannelCompactionProxyCoalescingTests {
    private let channel = RecordingOutgoingMessageChannel()
    private let recorder = RecordingOutgoingChannelDelegate()
    private let delegate: AnyOutgoingMessageChannelDelegate<TestMessage>
    private let proxy: OutgoingChannelCompactionProxy<TestMessage>

    init() {
        delegate = AnyOutgoingMessageChannelDelegate(recorder)
        proxy = Self.makeProxy(channel: channel, decision: .ignore)
        proxy.delegate = delegate
    }

    @Test func identicalReceiptIsAcknowledgedWithoutReachingChannel() {
        proxy.activate(restoringState: [:])

        proxy.addMessagesToQueue([TestMessage(id: 100)])
        proxy.addMessagesToQueue([TestMessage(id: 101)])

        #expect(channel.addedMessageBatches.map { $0.map(\.id) } == [[100]])
        #expect(recorder.addToQueueCalls.count == 1)
        #expect(recorder.addToQueueCalls.first?.message.id == 101)
        #expect(recorder.addToQueueCalls.first?.error == nil)
    }

    @Test func replacedReceiptIsDroppedFromTracking() {
        let replacingChannel = RecordingOutgoingMessageChannel()
        let replacingProxy = Self.makeProxy(channel: replacingChannel, decision: .replace)
        replacingProxy.activate(restoringState: [:])

        replacingProxy.addMessagesToQueue([TestMessage(id: 100)])
        replacingProxy.addMessagesToQueue([TestMessage(id: 101)])
        replacingProxy.addMessagesToQueue([TestMessage(id: 100)])

        #expect(replacingChannel.addedMessageBatches.map { $0.map(\.id) } == [[100], [101], [100]])
    }

    @Test(.timeLimit(.minutes(1)))
    func receiptIsKeptOutOfCompactedBatch() async {
        let result = await Self.compactMessages(
            withIds: [1, 2, 100],
            coalescer: AnyMessageCoalescer(StubMessageCoalescer(coalescableIds: [100], fixedDecision: .append))
        )

        #expect(result.compactedIds == [[1, 2]])
        #expect(result.channelIds == [[1], [2], [100], [999]])
    }

    @Test(.timeLimit(.minutes(1)))
    func defaultPolicyCompactsEverything() async {
        let result = await Self.compactMessages(withIds: [1, 2, 100], coalescer: AnyMessageCoalescer.none)

        #expect(result.compactedIds == [[1, 2, 100]])
        #expect(result.channelIds == [[1], [2], [999]])
    }
}

private extension OutgoingChannelCompactionProxyCoalescingTests {
    static func makeProxy(
        channel: RecordingOutgoingMessageChannel,
        decision: MessageCoalescingDecision = .ignore,
        coalescer: AnyMessageCoalescer<TestMessage>? = nil,
        compacter: AnyMessageCompactor<TestMessage> = AnyMessageCompactor(StubMessageCompactor()),
        sizeValidator: OutgoingRequestSizeValidating = FixedSizeValidator(maxPayloadSize: 1_000)
    ) -> OutgoingChannelCompactionProxy<TestMessage> {
        OutgoingChannelCompactionProxy(
            channel: AnyOutgoingMessageChannel(channel),
            compacter: compacter,
            coalescer: coalescer ?? AnyMessageCoalescer(
                StubMessageCoalescer(coalescableIds: [100, 101], fixedDecision: decision)
            ),
            routeSelector: PeerSessionRouteSelector { $0.route },
            sizeValidator: sizeValidator,
            statementDataCoder: FakeStatementDataCoder(headerSize: 10, messageSize: 10),
            workQueue: DispatchQueue(label: "test.outgoing-channel-compaction-proxy"),
            logger: nil
        )
    }

    static func compactMessages(
        withIds ids: [UInt32],
        coalescer: AnyMessageCoalescer<TestMessage>
    ) async -> (compactedIds: [[UInt32]], channelIds: [[UInt32]]) {
        let channel = RecordingOutgoingMessageChannel()
        let compactor = RecordingMessageCompactor(compactedMessage: TestMessage(id: 999))
        let proxy = makeProxy(
            channel: channel,
            coalescer: coalescer,
            compacter: AnyMessageCompactor(compactor),
            sizeValidator: FixedSizeValidator(maxPayloadSize: 35)
        )
        proxy.activate(restoringState: [:])

        await withCheckedContinuation { continuation in
            channel.onAddMessages = { messages in
                if messages.contains(where: { $0.id == 999 }) {
                    continuation.resume()
                }
            }

            for id in ids {
                proxy.addMessagesToQueue([TestMessage(id: id)])
            }
        }

        return (
            compactor.compactedMessageBatches.map { $0.map(\.id) },
            channel.addedMessageBatches.map { $0.map(\.id) }
        )
    }
}
