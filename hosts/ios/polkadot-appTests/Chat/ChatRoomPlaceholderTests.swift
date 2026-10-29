import Foundation
import Testing
@testable import polkadot_app

/// The room screen shows the placeholder of a product room only while the screen shows no message. A room whose
/// first page is on its way, or whose product does not answer, does not know yet that it has no messages.
struct ChatRoomPlaceholderTests {
    @Test func theRoomScreenShowsThePlaceholderOnlyWithoutMessages() {
        let metadata = metadata()

        #expect(metadata.placeholder(showsMessages: false, readiness: .ready) == "No games yet")
        #expect(metadata.placeholder(showsMessages: true, readiness: .ready) == nil)
    }

    @Test(arguments: [ChatRoomReadiness.loading, .unavailable])
    func aRoomThatIsNotReadyShowsNoPlaceholder(readiness: ChatRoomReadiness) {
        #expect(metadata().placeholder(showsMessages: false, readiness: readiness) == nil)
    }
}

private extension ChatRoomPlaceholderTests {
    func metadata() -> MessageListMetadata {
        var chatMetadata = ChatMetadata(
            chatId: .chatExtension("game.dot", roomId: "lobby"),
            peerMetadata: .unknown,
            state: .created
        )
        chatMetadata.roomPlaceholder = "No games yet"
        return MessageListMetadata(chatMetadata: chatMetadata, myUsername: "me")
    }
}
