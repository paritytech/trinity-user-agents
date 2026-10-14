import Foundation

struct ChatMetadata: Equatable {
    enum State: Equatable {
        case created
        case pending
    }

    let chatId: Chat.Id
    let peerMetadata: Chat.PeerMetadata
    let state: State
    /// The line under the name in the header, for example the description of a product room.
    var subtitle: String?
}
