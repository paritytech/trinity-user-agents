@testable import MessageExchangeKit

struct StubMessageCoalescer: MessageCoalescing {
    typealias Message = TestMessage

    let coalescableIds: Set<UInt32>
    let fixedDecision: MessageCoalescingDecision

    func canCoalesce(_ message: TestMessage) -> Bool {
        coalescableIds.contains(message.id)
    }

    func decision(for _: TestMessage, against _: TestMessage) -> MessageCoalescingDecision {
        fixedDecision
    }
}
