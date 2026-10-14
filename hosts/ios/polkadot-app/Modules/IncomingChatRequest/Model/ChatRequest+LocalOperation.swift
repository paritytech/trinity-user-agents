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
        let messageExchangeMode: MessageExchangeMode
        let incomingRequest: ChatRequest.ValidatedRemoteModel?
        let acceptorDevice: Chat.PeerDevice?
    }

    enum AcceptIncoming {
        case existing(
            requestId: String,
            messageExchangeMode: MessageExchangeMode,
            acceptorDevice: Chat.PeerDevice?
        )
        case new(
            ChatRequest.ValidatedRemoteModel,
            messageExchangeMode: MessageExchangeMode,
            acceptorDevice: Chat.PeerDevice?
        )

        var requestId: String {
            switch self {
            case let .existing(requestId, _, _):
                requestId
            case let .new(validatedRemoteModel, _, _):
                validatedRemoteModel.requestId
            }
        }
    }

    struct DeclineIncoming {
        let requestId: String
    }
}
