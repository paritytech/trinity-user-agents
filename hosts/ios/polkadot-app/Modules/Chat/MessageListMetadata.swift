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
}
