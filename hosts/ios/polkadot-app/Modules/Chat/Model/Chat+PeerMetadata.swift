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
        let contactSource: Chat.Contact.Source
        let icon: Icon
        let input: PeerMetadataInput
        let moreActions: [Chat.PeerAction]
    }
}

struct ChatWithPeerMetadata {
    let chat: Chat.LocalModel
    let peerMetadata: Chat.PeerMetadata
}

extension Chat.PeerMetadata {
    static var unknown: Chat.PeerMetadata {
        Chat.PeerMetadata(
            name: "Unknown",
            contactSource: .chat,
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
    }
}
