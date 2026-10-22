import Foundation

extension Chat {
    enum PeerMetadataInput: Equatable {
        struct InputField: Equatable {
            let canPay: Bool
            let canAttachFile: Bool
        }

        case inputField(InputField)
        case incomingRequest
        case outgoingRequest
        case blockedUser
        case empty

        var isInputField: Bool {
            switch self {
            case .inputField:
                true
            default:
                false
            }
        }
    }

    struct PeerMetadata: Equatable {
        let name: String
        let icon: Icon
        let input: PeerMetadataInput
        let moreActions: [Chat.PeerAction]
    }

    /// What a chat row shows for its peer. Input and actions belong to the chat screen, so a
    /// peer can have list metadata before it can answer, as a product whose bot is not built.
    struct PeerListMetadata: Equatable {
        let name: String
        let icon: PeerMetadata.Icon
    }
}

struct ChatWithPeerMetadata {
    let chat: Chat.LocalModel
    let peerMetadata: Chat.PeerListMetadata
}

extension Chat.PeerMetadata {
    var list: Chat.PeerListMetadata {
        Chat.PeerListMetadata(name: name, icon: icon)
    }
}

extension Chat.PeerMetadata {
    static var unknown: Chat.PeerMetadata {
        Chat.PeerMetadata(
            name: "Unknown",
            icon: .image(nil),
            input: .empty,
            moreActions: []
        )
    }
}

extension Chat.PeerMetadata {
    enum Icon: Equatable {
        case image(Data?)
        case bot
        /// The icon the product's manifest declares, loaded on demand.
        case product(domain: String)
    }
}
