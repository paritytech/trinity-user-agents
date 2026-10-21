import Foundation
import Testing
@testable import polkadot_app

struct ProductChatLinkTests {
    /// The chat product answers `contact:<account hex>` in its home room with the room of that contact.
    @Test func aChatWithAnAccountIsARoomLinkOfTheChatProductsHomeRoom() throws {
        let account = Data(repeating: 0xB0, count: 32)

        let url = try #require(ProductChatLink.url(account: account, tld: "paseo"))
        let link = try #require(ChatRoomLink(url.absoluteString))

        #expect(link.productId == "chat.paseo")
        #expect(link.roomId == "chats")
        #expect(link.payload == "contact:" + String(repeating: "b0", count: 32))
    }
}
