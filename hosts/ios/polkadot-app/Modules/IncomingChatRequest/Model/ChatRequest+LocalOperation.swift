import Foundation
import MessageExchangeKit

extension ChatRequest {
    struct NewIncoming {
        let remoteRequest: ChatRequest.ValidatedRemoteModel
        let remoteContact: Chat.RemoteContact
        let pushId: String?
        let ownKeyId: Chat.Contact.Own
    }

    struct NewOutgoing {
        let message: Chat.RequestMessage
        let remoteContact: Chat.RemoteContact
        let pushId: String?
        let ownKeyId: Chat.Contact.Own
    }

    struct UpdateIncoming {
        let remoteRequest: ChatRequest.ValidatedRemoteModel
    }

    struct ReplaceIncoming {
        let remoteRequest: ChatRequest.ValidatedRemoteModel
    }

    struct AcceptOutgoing {
        let requestId: String
        let incomingRequest: ChatRequest.ValidatedRemoteModel?
        /// Our device, advertised when two requests cross.
        let acceptorDevice: Chat.PeerDevice?
        /// The acceptor's `DeviceChatAccepted`, stored in the same transaction as the acceptance.
        let peerAcceptance: PeerAcceptance?
    }

    struct PeerAcceptance {
        let device: Chat.PeerDevice
        /// `nil` when the caller stores the message itself.
        let message: Chat.RemoteMessage?
    }

    enum AcceptIncoming {
        case existing(requestId: String, acceptorDevice: Chat.PeerDevice)
        case new(ChatRequest.ValidatedRemoteModel, acceptorDevice: Chat.PeerDevice)

        var requestId: String {
            switch self {
            case let .existing(requestId, _):
                requestId
            case let .new(validatedRemoteModel, _):
                validatedRemoteModel.requestId
            }
        }
    }

    struct DeclineIncoming {
        let requestId: String
    }

    struct CancelIncoming {
        let requestId: String
    }
}
