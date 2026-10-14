import Foundation

struct ChatCallModel {
    let callType: ChatCallType
    let contact: Chat.Contact
    let peerMetadata: Chat.PeerMetadata
    let isIncoming: Bool

    init(
        callType: ChatCallType,
        contact: Chat.Contact,
        peerMetadata: Chat.PeerMetadata,
        isIncoming: Bool = false
    ) {
        self.callType = callType
        self.contact = contact
        self.peerMetadata = peerMetadata
        self.isIncoming = isIncoming
    }
}
