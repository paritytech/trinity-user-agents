import Foundation

struct ChatMetadata: Equatable {
    enum State: Equatable {
        case created
        case pending
    }

    let chatId: Chat.Id
    let peerMetadata: Chat.PeerMetadata
    let state: State
}
