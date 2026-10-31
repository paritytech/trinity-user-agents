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

    /// The product keeps the messages of the room.
    var isProductSource: Bool {
        chatMetadata.roomPolicy?.messageStore == .product
    }

    /// The text that the room screen shows: the placeholder of the room while the screen shows no message.
    /// A product-source room shows it only after the product gives the first page.
    func placeholder(showsMessages: Bool, readiness: ChatRoomReadiness) -> String? {
        showsMessages || readiness != .ready ? nil : chatMetadata.roomPlaceholder
    }
}
