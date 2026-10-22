import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The edit history of a product-source room draws as the edit history of the native chat, and a contact opens
/// through the contact link of the chat product.
struct ProductEditHistoryTests {
    @Test func theNewestTextIsTheCurrentMessageAndTheOriginalComesLast() throws {
        let interactor = makeInteractor(status: .outgoing(.delivered))

        let result = try #require(interactor.result(of: [
            ChatMessageRevision(editedAt: 1_000, text: "one"),
            ChatMessageRevision(editedAt: 2_000, text: "two"),
            ChatMessageRevision(editedAt: 3_000, text: "three")
        ]))

        #expect(result.currentMessage.text == "three")
        #expect(result.currentMessage.timestamp == Date.fromChatTimestamp(3_000))
        #expect(result.currentMessage.outboxStatus != nil)
        let historyTimes = [Date.fromChatTimestamp(3_000), Date.fromChatTimestamp(2_000)]
        #expect(result.historyItems.map(\.timestamp) == historyTimes)
        #expect(result.originalTimestamp == Date.fromChatTimestamp(1_000))
        #expect(result.isOutgoing)
    }

    @Test func anIncomingMessageHasNoTick() throws {
        let interactor = makeInteractor(status: .incoming(.seen))

        let result = try #require(interactor.result(of: [ChatMessageRevision(editedAt: 1_000, text: "only")]))

        #expect(result.currentMessage.outboxStatus == nil)
        #expect(result.historyItems.isEmpty)
        #expect(!result.isOutgoing)
    }

    @Test func noRevisionDrawsNothing() {
        #expect(makeInteractor(status: .incoming(.seen)).result(of: []) == nil)
    }

    @MainActor @Test func theContactLinkNamesTheAccountInHex() {
        let account = Data(repeating: 0xAB, count: 32)

        #expect(ProductContactChatOpener.payload(account: account) == "contact:" + String(repeating: "ab", count: 32))
        #expect(ProductContactChatOpener.linkRoomId == "chats")
    }

    private func makeInteractor(status: Chat.LocalMessage.Status) -> ProductEditHistoryInteractor {
        ProductEditHistoryInteractor(
            request: ProductChatEditHistoryQueryRequest(roomId: ProductRoomTestData.roomId, messageId: "m1"),
            status: status,
            room: RecordingProductRoom(),
            logger: StubLogger()
        )
    }
}
