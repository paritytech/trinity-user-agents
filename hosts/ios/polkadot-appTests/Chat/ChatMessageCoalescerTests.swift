import Testing

@testable import polkadot_app

struct ChatMessageCoalescerTests {
    private let coalescer = ChatMessageCoalescer()

    @Test func identicalSeenReceiptsAreIgnored() {
        let pending = makeSeen(messageId: "m1", timestamp: 10)
        let incoming = makeSeen(messageId: "m1", timestamp: 10)

        #expect(coalescer.decision(for: incoming, against: pending) == .ignore)
    }

    @Test func differentSeenReceiptsAreReplaced() {
        let pending = makeSeen(messageId: "m1", timestamp: 10)
        let incoming = makeSeen(messageId: "m2", timestamp: 10)

        #expect(coalescer.decision(for: incoming, against: pending) == .replace)
    }

    @Test func seenReceiptAgainstTextMessageIsAppended() {
        let pending = makeMessage(.text("hi"))
        let incoming = makeSeen(messageId: "m1", timestamp: 10)

        #expect(coalescer.decision(for: incoming, against: pending) == .append)
    }

    @Test func seenReceiptsAreCoalescable() {
        #expect(coalescer.canCoalesce(makeSeen(messageId: "m1", timestamp: 10)))
    }

    @Test func textMessagesAreNotCoalescable() {
        #expect(!coalescer.canCoalesce(makeMessage(.text("hi"))))
    }
}

private extension ChatMessageCoalescerTests {
    func makeSeen(messageId: String, timestamp: UInt64) -> Chat.OpaqueMessage {
        makeMessage(.seen(.init(messageId: messageId, timestamp: timestamp)))
    }

    func makeMessage(_ content: ChatRemoteMessageContent) -> Chat.OpaqueMessage {
        Chat.OpaqueMessage(remoteMessage: Chat.RemoteMessage.newMessage(with: content))
    }
}
