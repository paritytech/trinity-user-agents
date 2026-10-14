import Foundation

enum MixnetUploadEvent {
    struct Progress {
        let uploaded: Int
        let total: Int
    }

    case onProgress(Progress)
    case onComplete(Chat.LocalMessage.Content.FileUploadingInfo)
    case onFailure(Error)

    func toAttachmentProgressEvent() -> AttachmentProgressEvent {
        switch self {
        case let .onProgress(progress):
            .onProgress(.init(loaded: progress.uploaded, total: progress.total))
        case .onComplete:
            .onComplete
        case let .onFailure(error):
            .onFailure(error)
        }
    }
}
