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

    /// The seed of the color of the avatar. A room with a contact has the color of the native chat with that
    /// account.
    var colorSeed: String {
        (roomPolicy?.contact?.personChatId ?? chatId).colorSeed
    }
}
