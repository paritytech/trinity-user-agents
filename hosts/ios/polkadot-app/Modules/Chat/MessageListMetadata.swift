import Foundation

struct MessageListMetadata {
    let chatMetadata: ChatMetadata
    let myUsername: String

    var peerMetadata: Chat.PeerMetadata {
        chatMetadata.peerMetadata
    }

    var peerName: String {
        chatMetadata.peerMetadata.name
    }

    /// The text that the room screen shows: the placeholder of the room while the screen shows no message.
    func placeholder(showsMessages: Bool) -> String? {
        showsMessages ? nil : chatMetadata.roomPlaceholder
    }
}
