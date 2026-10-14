import Foundation

enum ChatCallType {
    case audio
    case video
}

extension ChatCallType {
    init(remoteType: Chat.RemoteMessageContentV1.MessageContent.DataChannelPurpose) {
        switch remoteType {
        case .audio:
            self = .audio
        case .video:
            self = .video
        }
    }

    func toRemote() -> Chat.RemoteMessageContentV1.MessageContent.DataChannelPurpose {
        switch self {
        case .audio:
            .audio
        case .video:
            .video
        }
    }
}
