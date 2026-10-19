#if TESTNET_FEATURE
    import Foundation
    import Testing
    import TrUAPIHost
    @testable import polkadot_app

    /// The room that the mock product keeps answers `messages_query` and `search_query` as a product must, so the
    /// window of the room screen fills without gaps or repeats.
    struct MockChatConversationTests {
        private let conversation = MockChatConversation(messages: (1 ... 7).map { number in
            ChatMessage(
                messageId: "m\(number)",
                origin: ChatMessageOrigin(participantId: "product"),
                timestamp: UInt64(number),
                body: .text(text: number.isMultiple(of: 2) ? "even \(number)" : "odd \(number)"),
                status: .sent,
                replyTo: nil,
                editedAt: nil,
                reactions: []
            )
        })

        /// The host asks the newest page, then each older page with the cursor of the last one.
        @Test func pagesBackwardsCoverTheRoomOnceAndEnd() {
            let newest = conversation.page(.before(cursor: nil), size: 3)
            let older = conversation.page(.before(cursor: newest.next), size: 3)
            let oldest = conversation.page(.before(cursor: older.next), size: 3)

            #expect(ids(newest.messages) == ["m5", "m6", "m7"])
            #expect(ids(older.messages) == ["m2", "m3", "m4"])
            #expect(ids(oldest.messages) == ["m1"])
            #expect(oldest.next == nil)
        }

        @Test func pagesForwardsStartAtTheOldestAndEnd() {
            let first = conversation.page(.after(cursor: nil), size: 4)
            let second = conversation.page(.after(cursor: first.next), size: 4)

            #expect(ids(first.messages) == ["m1", "m2", "m3", "m4"])
            #expect(ids(second.messages) == ["m5", "m6", "m7"])
            #expect(second.next == nil)
        }

        @Test func searchGivesTheNewestHitsFirstAndPages() {
            let first = conversation.search("EVEN", cursor: nil, size: 2)
            let second = conversation.search("even", cursor: first.next, size: 2)

            #expect(ids(first.hits) == ["m6", "m4"])
            #expect(ids(second.hits) == ["m2"])
            #expect(second.next == nil)
            #expect(conversation.search("  ", cursor: nil, size: 2).hits.isEmpty)
        }

        /// A second reaction of the user with the same emoji must not count twice, and removing the last one removes
        /// the summary, or the room would draw a chip with a count of zero.
        @Test func theUserReactsOnceAndTheEmptySummaryGoes() {
            var conversation = conversation

            conversation.react("m1", emoji: "👍", placed: true, byUser: true)
            conversation.react("m1", emoji: "👍", placed: true, byUser: true)
            conversation.react("m1", emoji: "👍", placed: true, byUser: false)
            #expect(conversation.message("m1")?.reactions == [
                ChatReactionSummary(emoji: "👍", count: 2, reactedByUser: true)
            ])

            conversation.react("m1", emoji: "👍", placed: false, byUser: true)
            #expect(conversation.message("m1")?.reactions == [
                ChatReactionSummary(emoji: "👍", count: 1, reactedByUser: false)
            ])

            conversation.react("m1", emoji: "👍", placed: false, byUser: false)
            #expect(conversation.message("m1")?.reactions == [])
        }

        @Test func anActionOnAMessageThatIsGoneIsRefused() {
            var conversation = conversation

            let removed = conversation.remove("m3")
            let edited = conversation.edit("m3", body: .text(text: "late"), at: 9)
            let marked = conversation.setStatus("m3", .read)

            #expect(removed)
            #expect(!edited)
            #expect(!marked)
        }

        private func ids(_ messages: [ChatMessage]) -> [String] {
            messages.map(\.messageId)
        }
    }
#endif
