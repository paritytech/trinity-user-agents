import Foundation

struct ChatListModel {
    let establishedChats: [ChatWithPeerMetadata]
    let pendingIncomingRequestCount: Int
    let newIncomingRequestCount: Int
}
