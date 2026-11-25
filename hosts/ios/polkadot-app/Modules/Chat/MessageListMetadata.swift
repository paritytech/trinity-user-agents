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

    func isSeenByPeer(_ message: Chat.LocalMessage) -> Bool {
        message.order > 0 && message.order <= chatMetadata.peerSeenOrder
    }
}

extension MessageListMetadata {
    /// The peer cancelled the request and the chat is gone; only the cancelled banner remains, with no actions.
    func withCancelledRequest() -> MessageListMetadata {
        let peer = Chat.PeerMetadata(
            name: peerMetadata.name,
            icon: peerMetadata.icon,
            input: .cancelledRequest,
            moreActions: []
        )
        let chat = ChatMetadata(
            chatId: chatMetadata.chatId,
            peerMetadata: peer,
            state: chatMetadata.state,
            peerSeenOrder: chatMetadata.peerSeenOrder
        )

        return MessageListMetadata(chatMetadata: chat, myUsername: myUsername)
    }
}
