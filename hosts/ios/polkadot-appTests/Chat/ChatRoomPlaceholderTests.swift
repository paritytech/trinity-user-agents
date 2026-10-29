import Foundation
import Testing
@testable import polkadot_app

/// The room screen shows the placeholder of a product room only while the screen shows no message.
struct ChatRoomPlaceholderTests {
    @Test func theRoomScreenShowsThePlaceholderOnlyWithoutMessages() {
        var chatMetadata = ChatMetadata(
            chatId: .chatExtension("game.dot", roomId: "lobby"),
            peerMetadata: .unknown,
            state: .created
        )
        chatMetadata.roomPlaceholder = "No games yet"
        let metadata = MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me")

        #expect(metadata.placeholder(showsMessages: false) == "No games yet")
        #expect(metadata.placeholder(showsMessages: true) == nil)
    }
}
