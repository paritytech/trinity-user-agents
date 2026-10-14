import Foundation
import AVFoundation

struct VoIPCallKitInput {
    let name: String
    let callType: ChatCallType

    var hasVideo: Bool {
        switch callType {
        case .audio: false
        case .video: true
        }
    }
}

struct VoIPCallKitData {
    let uuid: UUID
    let status: VoIPCallKitStatus
    let pendingAnswerOrEndActionSource: VoIPCallKitActionSource?
    let pendingMutedActionSource: VoIPCallKitActionSource?

    func updatingStatus(_ status: VoIPCallKitStatus) -> Self {
        .init(
            uuid: uuid,
            status: status,
            pendingAnswerOrEndActionSource: pendingAnswerOrEndActionSource,
            pendingMutedActionSource: pendingMutedActionSource
        )
    }

    func updatingPendingAnswerOrEndActionSource(_ pendingAnswerOrEndActionSource: VoIPCallKitActionSource?) -> Self {
        .init(
            uuid: uuid,
            status: status,
            pendingAnswerOrEndActionSource: pendingAnswerOrEndActionSource,
            pendingMutedActionSource: pendingMutedActionSource
        )
    }

    func updatingPendingMutedActionSource(_ pendingMutedActionSource: VoIPCallKitActionSource?) -> Self {
        .init(
            uuid: uuid,
            status: status,
            pendingAnswerOrEndActionSource: pendingAnswerOrEndActionSource,
            pendingMutedActionSource: pendingMutedActionSource
        )
    }
}

enum VoIPCallKitStatus {
    case initiallyReported
    case reported(VoIPCallKitInput)
    case connected(VoIPCallKitInput)

    var input: VoIPCallKitInput? {
        switch self {
        case .initiallyReported:
            nil
        case let .reported(input),
             let .connected(input):
            input
        }
    }
}

enum VoIPCallKitActionSource {
    case app
    case system
}
