import Foundation

struct ChatMetadata: Equatable {
    enum State: Equatable {
        case created
        case pending
    }

    let chatId: Chat.Id
    let peerMetadata: Chat.PeerMetadata
    let state: State
    /// What a product room offers. Nil for a person chat.
    var roomPolicy: ChatRoomPolicy?
    /// The text that a product room shows while it has no messages. Nil shows nothing.
    var roomPlaceholder: String?
}
