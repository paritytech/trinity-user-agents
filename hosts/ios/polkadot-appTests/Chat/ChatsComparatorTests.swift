import Foundation
import Testing
@testable import polkadot_app

/// The chat list orders its rows by the order of their last message first, then by time. A room whose product keeps
/// the messages has no local order, so its row compares by time.
struct ChatsComparatorTests {
    @Test func anOrderedMessageComesBeforeAnUnorderedOne() {
        let ordered = chat("ordered", timestamp: 1_000, order: 5)
        let unordered = chat("unordered", timestamp: 2_000, order: 0)

        #expect(ChatsComparator.lastMessageComparator(chat1: ordered, chat2: unordered))
        #expect(!ChatsComparator.lastMessageComparator(chat1: unordered, chat2: ordered))
    }

    @Test func aRoomThatTheProductKeepsComparesByTime() {
        let ordered = chat("ordered", timestamp: 1_000, order: 5)
        let productRoom = chat("product", timestamp: 2_000, order: 0, productSource: true)

        #expect(ChatsComparator.lastMessageComparator(chat1: productRoom, chat2: ordered))
        #expect(!ChatsComparator.lastMessageComparator(chat1: ordered, chat2: productRoom))
    }
}

private extension ChatsComparatorTests {
    func chat(
        _ roomId: String,
        timestamp: Chat.Timestamp,
        order: UInt64,
        productSource: Bool = false
    ) -> Chat.LocalModel {
        let chatId = Chat.Id.chatExtension("game.dot", roomId: roomId)
        let config = Chat.RoomConfig(productSource: productSource, reactionDetails: false, reportSeen: false)

        return Chat.LocalModel(
            peer: .chatExtension("game.dot", roomId: roomId),
            message: Chat.LocalMessage(
                messageId: roomId,
                chatId: chatId,
                origin: .chatExtension("game.dot"),
                creationSource: .localDevice,
                status: .incoming(.seen),
                timestamp: timestamp,
                content: .text(roomId),
                reactions: [],
                compactionId: nil,
                relatedMessages: [],
                order: order
            ),
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomDeclarations: Chat.RoomDeclarations(config: config, configGeneration: 0, preview: .hostDefault)
        )
    }
}
